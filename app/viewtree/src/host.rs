//! A program as a host page drives it: flags and inputs as JSON, events by id, patches as bytes,
//! effects as JSON. [`export!`](crate::export) makes it a wasm-bindgen class.

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::node::On;
use crate::program::{Frame, Program, Runtime};

pub trait Hosted: Program<Flags: DeserializeOwned, Effect: Serialize> {
    type Input: DeserializeOwned;
    fn input(input: Self::Input) -> Self::Msg;
    /// An input whose answer the host hands over beside it as text, in UTF-8: the program parses
    /// that text once, into the type that reads it, and no answer is printed inside an input's JSON.
    type Reply: DeserializeOwned;
    fn reply(reply: Self::Reply, text: Vec<u8>) -> Self::Msg;

    /// `{"flags": …, "inputs": […]}` that a worker hands the page's class on the same module
    /// (`web/src/pages/rehearse.ts`), so the code a page's first readings run, its class's
    /// exports included, is compiled before the page needs it.
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

    /// A bad reply steps nothing; its text is the program's to parse.
    pub fn reply(&mut self, reply: &str, text: Vec<u8>) -> Result<Vec<u8>, String> {
        let reply = serde_json::from_str::<serde_json::Value>(reply)
            .and_then(serde_json::from_value)
            .map_err(|e| format!("reply: {e}"))?;
        let frame = self.runtime.send(P::reply(reply, text));
        Ok(self.took(frame))
    }

    /// `None` when no handler took it, so the painter leaves it to the browser.
    pub fn event(&mut self, id: u32, on: u8, words: String) -> Option<Vec<u8>> {
        let frame = self.runtime.event(id, On::of(on)?, words)?;
        Some(self.took(frame))
    }

    #[must_use]
    pub fn fresh(&self) -> Vec<u8> {
        self.runtime.fresh()
    }

    /// The effects since the host last took them, as a JSON array.
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
/// `input(json)`, `reply(json, text)`, `event(id, on, words)`, `fresh()`, `effects()`, and the static `rehearsal()`.
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

            pub fn reply(
                &mut self,
                reply: &str,
                text: Vec<u8>,
            ) -> Result<Vec<u8>, wasm_bindgen::JsError> {
                self.host
                    .reply(reply, text)
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

            pub fn rehearsal() -> Option<String> {
                <$program as $crate::host::Hosted>::rehearsal().map(str::to_owned)
            }
        }
    };
}
