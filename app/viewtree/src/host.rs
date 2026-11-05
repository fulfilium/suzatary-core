//! A program as a host page drives it: flags and inputs as JSON, events by id, patches as bytes,
//! effects as JSON. [`export!`](crate::export) makes it a wasm-bindgen class.

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::node::On;
use crate::program::{Frame, Program, Runtime};

pub trait Hosted: Program<Flags: DeserializeOwned, Effect: Serialize> {
    type Input: DeserializeOwned;
    fn input(input: Self::Input) -> Self::Msg;
}

pub struct Host<P: Hosted> {
    runtime: Runtime<P>,
    effects: Vec<P::Effect>,
}

impl<P: Hosted> Host<P> {
    pub fn start(flags: &str) -> Result<(Self, Vec<u8>), String> {
        let flags = serde_json::from_str(flags).map_err(|e| format!("flags: {e}"))?;
        let (runtime, Frame { patches, effects }) = Runtime::start(flags);
        Ok((Self { runtime, effects }, patches))
    }

    /// A bad input steps nothing.
    pub fn input(&mut self, input: &str) -> Result<Vec<u8>, String> {
        let input = serde_json::from_str(input).map_err(|e| format!("input: {e}"))?;
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

    fn took(&mut self, frame: Frame<P::Effect>) -> Vec<u8> {
        self.effects.extend(frame.effects);
        frame.patches
    }
}

/// A wasm-bindgen class `$class` over `$program`: `new(flags)`, `first()` (the first paint),
/// `input(json)`, `event(id, on, words)`, `fresh()`, `effects()`. Needs `wasm-bindgen`.
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
        }
    };
}
