mod buffer;
mod catalog;
mod common;
mod storage;
mod tuple;

use std::{
    io::{BufRead, BufReader, BufWriter, Write},
    net::{Shutdown, TcpListener, TcpStream},
    sync::RwLock,
    thread,
};

use anyhow::{Context, Result};
use buffer::buffer_manager::BufferManager;
use catalog::Catalog;
use clap::Parser;
use storage::file_manager::FileManager;

#[derive(Parser)]
struct ServerConfig {
    #[arg(long, help = "Directory where data is stored")]
    data: String,

    #[arg(
        long,
        help = "If enabled, it assumes that data directory is empty and needs to be initialized"
    )]
    new: bool,

    #[arg(long, default_value_t = 42666)]
    port: u16,

    #[arg(long, default_value_t = 8, help = "Size of buffer pool")]
    pool_size: usize,
}

fn trim_newline(s: &mut String) {
    let len = s.len();
    if s.ends_with("\r\n") {
        s.truncate(len - 2);
    } else if s.ends_with('\n') {
        s.truncate(len - 1);
    }
}

fn handle_client(mut stream: TcpStream, catalog: &RwLock<Catalog>) -> Result<()> {
    stream.write_all("Welcome to erdb\n".as_bytes())?;
    stream.write_all("> ".as_bytes())?;
    stream.flush()?;

    let mut reader = BufReader::new(&stream);
    let mut writer = BufWriter::new(&stream);
    let mut line = String::new();

    loop {
        reader.read_line(&mut line)?;

        if line.as_bytes().is_empty() {
            // Client didn't send anything.
            return Ok(());
        } else {
            trim_newline(&mut line);
            if line.eq(".exit") {
                break;
            } else if line.eq(".tables") {
                let catalog = catalog.read().unwrap();
                let tables = catalog.list_tables();
                writer.write_all(tables.join(" ").as_bytes())?;
            } else {
                writer.write_all(format!("Unknown command: {line}").as_bytes())?;
            }
        }

        line.clear();
        writer.write_all("\n> ".as_bytes())?;
        writer.flush()?;
    }

    stream.shutdown(Shutdown::Both)?;
    Ok(())
}

fn main() -> Result<()> {
    let config = ServerConfig::parse();

    let file_manager = FileManager::new(config.data)?;
    let buffer_manager = BufferManager::new(file_manager, config.pool_size);

    let catalog = RwLock::new(
        Catalog::new(&buffer_manager, config.new)
            .with_context(|| "Failed to create catalog".to_string())?,
    );
    let listener = TcpListener::bind(("localhost", config.port))?;

    thread::scope(|scope| {
        let catalog = &catalog;

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    scope.spawn(move || match handle_client(stream, catalog) {
                        Ok(()) => (),
                        Err(e) => println!("Failed to handle client. Cause: {e}"),
                    });
                }
                Err(e) => println!("Could not get tcp stream: {e}"),
            }
        }
    });

    Ok(())
}