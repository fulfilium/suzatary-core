//! How the box reaches out: a subprocess described as data (`run`), a program on `PATH`
//! (`tool`), HTTP over rustls (`http`, `tokens`), the secret store (`secrets`) and what is read
//! through it (`github_tokens`, `routine`), and a checkout on disk (`repository`).

#![forbid(unsafe_code)]

pub mod github_tokens;
pub mod http;
pub mod repository;
pub mod routine;
pub mod run;
pub mod secrets;
pub mod tokens;
pub mod tool;
