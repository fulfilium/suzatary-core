//! The fence's source: every mode tries one fence of the `source` world.

// The generated lifting rebuilds each list from the pointer and length its own allocator
// handed out, which this lint cannot see.
#[allow(clippy::same_length_and_capacity)]
mod bindings {
    #[cfg(not(feature = "escapee"))]
    wit_bindgen::generate!({
        path: ["../wit", "wit"],
        world: "suzatary:fence/fence",
        generate_all,
    });

    #[cfg(feature = "escapee")]
    wit_bindgen::generate!({
        path: ["../wit", "wit"],
        world: "suzatary:fence/escapee",
        generate_all,
    });
}

use bindings::exports::suzatary::plugin::sync::{Guest, Synced};
use bindings::suzatary::plugin::fetch::{self, Response, Target};

struct Fence;

impl Guest for Fence {
    fn sync(cursor: Option<Vec<u8>>, credentials: Vec<String>) -> Result<Synced, String> {
        let cursor = String::from_utf8(cursor.unwrap_or_default()).map_err(|_| "not text")?;
        let (mode, argument) = cursor.split_once(' ').unwrap_or((&cursor, ""));
        match mode {
            // The honest path, and a URL whose origin the manifest may not allow.
            "get" => lines(&fetch::fetch(&Target::Url(argument.to_owned()))?),
            // A credential named as the URL itself, whatever the manifest says of it.
            "credential" => lines(&fetch::fetch(&Target::Credential(argument.to_owned()))?),
            // A megabyte of words at every door a source has: a URL, a credential's name, and
            // the error it answers with.
            "megabyte" => {
                let words = "é".repeat(1 << 19);
                let _ = fetch::fetch(&Target::Url(words.clone()));
                let _ = fetch::fetch(&Target::Credential(words.clone()));
                Err(words)
            }
            // As many fetches as it can make, and how many the host let through.
            "flood" => {
                let sent = (0..200)
                    .filter(|_| fetch::fetch(&Target::Url(argument.to_owned())).is_ok())
                    .count();
                Ok(Synced {
                    items: vec![sent.to_string().into_bytes()],
                    cursor: b"done".to_vec(),
                })
            }
            // A sleep the clock would end after every budget.
            "sleep" => {
                bindings::wasi::clocks::monotonic_clock::subscribe_duration(u64::MAX).block();
                Err("woke".to_owned())
            }
            "loop" => spin(),
            "grow" => hoard(),
            "panic" => panic!("the fence plugin panics on purpose"),
            // The names it was lent, one item each: what it may designate, never a value.
            "lent" => Ok(Synced {
                items: credentials.into_iter().map(String::into_bytes).collect(),
                cursor: b"done".to_vec(),
            }),
            #[cfg(feature = "escapee")]
            "escape" => escape(),
            _ => Err(format!("no mode {mode:?}")),
        }
    }
}

bindings::export!(Fence with_types_in bindings);

/// One item per line of the body, and a cursor saying it is done.
fn lines(response: &Response) -> Result<Synced, String> {
    let body = std::str::from_utf8(&response.body).map_err(|_| "the body is not text")?;
    Ok(Synced {
        items: body.lines().map(|line| line.as_bytes().to_vec()).collect(),
        cursor: format!("done {}", response.status).into_bytes(),
    })
}

/// Never returns: only the host's deadline ends it.
fn spin() -> ! {
    let mut turns: u64 = 0;
    loop {
        turns = std::hint::black_box(turns.wrapping_add(1));
    }
}

/// Never returns: only the host's memory cap ends it.
fn hoard() -> ! {
    let mut hoard: Vec<Vec<u8>> = Vec::new();
    loop {
        hoard.push(std::hint::black_box(vec![7; 1 << 20]));
    }
}

#[cfg(feature = "escapee")]
fn escape() -> Result<Synced, String> {
    let directories = bindings::wasi::filesystem::preopens::get_directories();
    let network = bindings::wasi::sockets::instance_network::instance_network();
    Err(format!(
        "{} directories and network {network}",
        directories.len()
    ))
}
