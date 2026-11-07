//! A program as a host page drives it: flags and inputs as JSON, events by id, patches as bytes,
//! effects as JSON. [`export!`](crate::export) makes it a wasm-bindgen class.

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::node::On;
use crate::program::{Frame, Program, Runtime};

pub trait Hosted: Program<Flags: DeserializeOwned, Effect: Serialize> {
    type Input: DeserializeOwned;
    fn input(input: Self::Input) -> Self::Msg;

    /// A JSON document `{"flags": …, "inputs": […]}`, flags and inputs as the host sends them,
    /// that takes the program through the code its first real readings run. The host runs it in
    /// a worker on the same compiled module, so the browser compiles that code there, before a
    /// page's first input would compile it on the page's thread.
    #[must_use]
    fn rehearsal() -> Option<&'static str> {
        None
    }
}

pub struct Host<P: Hosted> {
    runtime: Runtime<P>,
    effects: Vec<P::Effect>,
}

impl<P: Hosted> Host<P> {
    pub fn start(flags: &str) -> Result<(Self, Vec<u8>), String> {
        let flags = serde_json::from_str::<serde_json::Value>(flags)
            .and_then(serde_json::from_value)
            .map_err(|e| format!("flags: {e}"))?;
        let (runtime, Frame { patches, effects }) = Runtime::start(flags);
        Ok((Self { runtime, effects }, patches))
    }

    /// A bad input steps nothing.
    pub fn input(&mut self, input: &str) -> Result<Vec<u8>, String> {
        let input = serde_json::from_str::<serde_json::Value>(input)
            .and_then(serde_json::from_value)
            .map_err(|e| format!("input: {e}"))?;
        let frame = self.runtime.send(P::input(input));
        Ok(self.took(frame))
    }

    /// `None` when no handler took it (an event code the painter does not send is taken by none),
    /// so the painter leaves it to the browser.
    pub fn event(&mut self, id: u32, on: u8, words: String) -> Option<Vec<u8>> {
        let frame = self.runtime.event(id, On::of(on)?, words)?;
        Some(self.took(frame))
    }

    #[must_use]
    pub fn fresh(&self) -> Vec<u8> {
        self.runtime.fresh()
    }

    /// A JSON array of the effects since the host last took them; fails on an effect that does
    /// not print as JSON (a map keyed by something other than a string).
    pub fn effects(&mut self) -> Result<String, String> {
        serde_json::to_string(&std::mem::take(&mut self.effects))
            .map_err(|e| format!("effects: {e}"))
    }

    /// The program's rehearsal run as a page would run it, and dropped: nothing for a program
    /// with none.
    pub fn rehearse() -> Result<(), String> {
        let Some(text) = P::rehearsal() else {
            return Ok(());
        };
        let Rehearsal { flags, inputs } = serde_json::from_str::<serde_json::Value>(text)
            .and_then(serde_json::from_value)
            .map_err(|e| format!("rehearsal: {e}"))?;
        let (mut host, _) = Self::start(&flags.to_string())?;
        for input in inputs {
            host.input(&input.to_string())?;
        }
        drop(host.fresh());
        host.effects().map(drop)
    }

    fn took(&mut self, frame: Frame<P::Effect>) -> Vec<u8> {
        self.effects.extend(frame.effects);
        frame.patches
    }
}

/// A program's rehearsal: the flags it starts with and the inputs a first visit hands it.
#[derive(serde::Deserialize)]
struct Rehearsal {
    flags: serde_json::Value,
    inputs: Vec<serde_json::Value>,
}

/// A wasm-bindgen class `$class` over `$program`: `new(flags)`, `first()` (the first paint),
/// `input(json)`, `event(id, on, words)`, `fresh()`, `effects()`, and the static `rehearse()`.
/// Needs `wasm-bindgen`.
#[macro_export]
macro_rules! export {
    ($class:ident = $program:ty) => {
        #[wasm_bindgen::prelude::wasm_bindgen]
        pub struct $class {
            host: $crate::host::Host<$program>,
            first: Vec<u8>,
        }

        #[wasm_bindgen::prelude::wasm_bindgen]
        impl $class {
            #[wasm_bindgen::prelude::wasm_bindgen(constructor)]
            pub fn new(flags: &str) -> Result<$class, wasm_bindgen::JsError> {
                let (host, first) =
                    $crate::host::Host::start(flags).map_err(|e| wasm_bindgen::JsError::new(&e))?;
                Ok($class { host, first })
            }

            pub fn first(&mut self) -> Vec<u8> {
                std::mem::take(&mut self.first)
            }

            pub fn input(&mut self, input: &str) -> Result<Vec<u8>, wasm_bindgen::JsError> {
                self.host
                    .input(input)
                    .map_err(|e| wasm_bindgen::JsError::new(&e))
            }

            pub fn event(&mut self, id: u32, on: u8, words: String) -> Option<Vec<u8>> {
                self.host.event(id, on, words)
            }

            pub fn fresh(&self) -> Vec<u8> {
                self.host.fresh()
            }

            pub fn effects(&mut self) -> Result<String, wasm_bindgen::JsError> {
                self.host
                    .effects()
                    .map_err(|e| wasm_bindgen::JsError::new(&e))
            }

            pub fn rehearse() -> Result<(), wasm_bindgen::JsError> {
                $crate::host::Host::<$program>::rehearse()
                    .map_err(|e| wasm_bindgen::JsError::new(&e))
            }
        }
    };
}
