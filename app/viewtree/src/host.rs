//! A program as a host page drives it: flags and inputs as JSON, events by id, patches as bytes,
//! effects as JSON. [`export!`](crate::export) makes it a wasm-bindgen class.

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::node::On;
use crate::program::{Frame, Program, Runtime};

/// A program a host can drive: what the host hands it, read once at the edge.
pub trait Hosted: Program<Flags: DeserializeOwned, Effect: Serialize> {
    /// What the host hands it.
    type Input: DeserializeOwned;
    /// The message an input is.
    fn input(input: Self::Input) -> Self::Msg;
}

/// A running program and the effects its last step asked for, until the host takes them.
pub struct Host<P: Hosted> {
    runtime: Runtime<P>,
    effects: Vec<P::Effect>,
}

impl<P: Hosted> Host<P> {
    /// Started from the host's flags, with the bytes of the first paint.
    ///
    /// # Errors
    /// The flags' JSON does not read as the program's flags.
    pub fn start(flags: &str) -> Result<(Self, Vec<u8>), String> {
        let flags = serde_json::from_str(flags).map_err(|e| format!("flags: {e}"))?;
        let (runtime, Frame { patches, effects }) = Runtime::start(flags);
        Ok((Self { runtime, effects }, patches))
    }

    /// An input stepped: the bytes of its patches.
    ///
    /// # Errors
    /// The JSON does not read as an input; nothing is stepped.
    pub fn input(&mut self, input: &str) -> Result<Vec<u8>, String> {
        let input = serde_json::from_str(input).map_err(|e| format!("input: {e}"))?;
        let frame = self.runtime.send(P::input(input));
        Ok(self.took(frame))
    }

    /// An event the painter reported on element `id`: the bytes of its patches; an event code
    /// the painter does not send patches nothing.
    pub fn event(&mut self, id: u32, on: u8, words: String) -> Vec<u8> {
        match On::of(on) {
            Some(on) => {
                let frame = self.runtime.event(id, on, words);
                self.took(frame)
            }
            None => Vec::new(),
        }
    }

    /// The effects asked for since the host last took them, as a JSON array.
    ///
    /// # Errors
    /// An effect that does not print as JSON (a map keyed by something other than a string).
    pub fn effects(&mut self) -> Result<String, String> {
        serde_json::to_string(&std::mem::take(&mut self.effects))
            .map_err(|e| format!("effects: {e}"))
    }

    fn took(&mut self, frame: Frame<P::Effect>) -> Vec<u8> {
        self.effects.extend(frame.effects);
        frame.patches
    }
}

/// A wasm-bindgen class named `$class` over the [`Hosted`] program `$program`: `new(flags)`,
/// `first()` (the first paint's bytes), `input(json)`, `event(id, on, words)` and `effects()`.
/// The calling crate depends on `wasm-bindgen`.
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

            pub fn event(&mut self, id: u32, on: u8, words: String) -> Vec<u8> {
                self.host.event(id, on, words)
            }

            pub fn effects(&mut self) -> Result<String, wasm_bindgen::JsError> {
                self.host
                    .effects()
                    .map_err(|e| wasm_bindgen::JsError::new(&e))
            }
        }
    };
}
