use easyread_gpui::{
    ai, formula,
    model::text_hash,
    store::{Library, apply_ops, empty_reader, write_json},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    sync::{Arc, atomic::AtomicBool},
    thread,
};

#[path = "support/math.rs"]
mod math_fixtures;
mod support;
use support::pdf_fixture;

#[path = "core/mod.rs"]
mod core_tests;
