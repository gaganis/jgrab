use std::net::{TcpStream, Shutdown};
use std::io::{Stdin, stdin, stdout, Read, Write, Error, Result, Cursor};
use std::fs::File;
use std::env;
use std::iter::Iterator;
use std::str;
use std::option::Option;
use std::process::{Command, Child, exit};
use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;
use wait_timeout::ChildExt;
use Input::*;

extern crate wait_timeout;

const MAX_RETRIES: usize = 5;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const JGRAB_INFO: &str = "\
=============== JGrab Client ================
 - https://github.com/renatoathaydes/jgrab -
=============================================
Jgrab can execute Java code from stdin (if not given any argument),
a Java file, or a Java snippet.

This is the native JGrab Client, written in Rust!

A Java daemon is started the first time the JGrab Client is run so
that subsequent runs are much faster.";

const JGRAB_USAGE: &str = "\
Usage:
  jgrab [<option> | java_file [java-args*] | -e java_snippet]
Options:
  --stop -s
    Stops the JGrab daemon.
  --start -t
    Starts the JGrab daemon (if not yet running).
  --help -h
    Shows usage.
  --version -v
    Shows version information.";

/// All possible sources of input for the JGrab Client
enum Input {
    FileInput(File),
    StdinInput(Stdin),
    TextInput(Cursor<String>),
    Copy(Box<Read>)
}

impl Read for Input {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        match *self {
            FileInput(ref mut r) => r.read(buf),
            StdinInput(ref mut r) => r.read(buf),
            TextInput(ref mut r) => r.read(buf),
            Copy(ref mut r) => r.read(buf)
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    let input: Input;

    if args.len() == 0 {
        // no args, pipe stdin
        input = StdinInput(stdin());
    } else if args.len() == 1 && !args[0].starts_with('-') {
        // there's one argument and it is not an option, so it must be a file
        input = file_input(&args[0]);
    } else if args.len() == 1 {
        // one argument starting with -
        match args[0].trim() {
            "--help" | "-h" => {
                println!("{}\n\n{}", JGRAB_INFO, JGRAB_USAGE);
                return
            }
            "--start" | "-t" => {
                send_message_retrying(TextInput(Cursor::new("-e null".to_string())));
                return
            }
            "--stop" | "-s" => {
                if let Err(_) = connect() {
                    log("daemon is not running");
                    return
                }
                input = TextInput(Cursor::new("--stop".to_string()))
            }
            "--version" | "-v" => {
                println!("JGrab Client Version: {}", VERSION);
                show_daemon_version();
                return
            }
            "-e" => usage_error(&format!("-e option missing code snippet")),
            _ => {
                usage_error(&format!("invalid option"));
            }
        }
    } else {
        // more than one argument given
        match args[0].trim() {
            "-e" => {
                // just pass on the arguments to JGrab
                input = create_message(&args)
            }
            _ => {
                // assume it's a file + java arguments
                let file = file_input(&args[0]);
                let java_args = create_wrapped_message("[", &args[1..], "]\n");
                input = Copy(Box::new(java_args.chain(file)))
            }
        }
    }

    send_message_retrying(input);
}

fn file_input(file_name: &String) -> Input {
    match File::open(file_name) {
        Ok(file) => FileInput(file),
        Err(err) => error(&format!("unable to read file: {}", err))
    }
}

fn create_message(args: &[String]) -> Input {
    TextInput(Cursor::new(args.join(" ")))
}

fn create_wrapped_message(prefix: &str, args: &[String], suffix: &str) -> Input {
    TextInput(Cursor::new(prefix.to_string() + &args.join(" ") + suffix))
}

fn send_message_retrying<R: Read>(mut reader: R) {
    if let Some(_) = send_message(&mut reader, false) {
        // failed to connect, try to start the daemon, then retry
        let mut retries = MAX_RETRIES;

        let mut child = start_daemon();
        check_status(&mut child);

        while retries > 0 {
            if let Some(err) = send_message(&mut reader, true) {
                check_status(&mut child);

                log(&format!("unable to connect to JGrab daemon: {}", err));
                log(&format!("will re-try {} more times", &mut retries));
                retries -= 1;
            } else {
                break // success
            }
        }

        if retries == 0 {
            error("unable to start JGrab daemon. \
                   Make sure JGrab's port [5002] is not already bound");
        }
    }
}

fn connect() -> Result<TcpStream> {
    TcpStream::connect("127.0.0.1:5002")
}

fn send_message<R: Read>(reader: &mut R,
                         is_retry: bool) -> Option<Error> {
    match connect() {
        Ok(mut stream) => {
            if is_retry {
                log("Connected!");
            }

            let mut socket_message = [0u8; 1024];

            loop {
                match reader.read(&mut socket_message) {
                    Ok(n) => {
                        if n == 0 {
                            break;
                        } else {
                            stream.write(&socket_message[0..n]).unwrap();
                        }
                    }
                    Err(err) => error(&err.to_string())
                }
            }

            stream.shutdown(Shutdown::Write).unwrap();

            let mut client_buffer = socket_message;

            loop {
                match stream.read(&mut client_buffer) {
                    Ok(n) => {
                        if n == 0 {
                            break;
                        } else {
                            stdout().write(&client_buffer[0..n]).unwrap();
                        }
                    }
                    Err(err) => error(&err.to_string())
                }
            }

            Option::None
        }
        Err(err) => Option::Some(err)
    }
}

fn start_daemon() -> Child {
    log("Starting daemon");

    if let Some(user_home) = env::home_dir() {
        let mut jgrab_jar: PathBuf = user_home;
        jgrab_jar.push(".jgrab");
        jgrab_jar.push("jgrab.jar");

        if jgrab_jar.as_path().is_file() {
            let cmd = Command::new("java")
                .arg("-jar")
                .arg(jgrab_jar.into_os_string().into_string().unwrap())
                .arg("--daemon")
                .spawn();

            match cmd {
                Ok(child) => {
                    log(&format!("Daemon started, pid={}", child.id()));
                    child
                }
                Err(err) => error(&err.to_string())
            }
        } else {
            error("The JGrab jar is not installed! Please install it as explained \
                   in https://github.com/renatoathaydes/jgrab");
        }
    } else {
        error("user.home could not be found, making it impossible to start the JGrab Daemon");
    }
}

fn check_status(child: &mut Child) {
    let timeout = Duration::from_secs(1);

    match child.wait_timeout(timeout) {
        Ok(Some(status)) => error(&format!(
            "The JGrab daemon has died prematurely, {}", status)),
        Ok(None) => {
            // give time for the socket server to become active
            sleep(Duration::from_secs(1));
        }
        Err(e) => error(&format!(
            "unable to wait for JGrab daemon process status: {}", e)),
    }
}

fn show_daemon_version() {
    if let Some(_) = send_message(&mut TextInput(Cursor::new("--version".to_string())), false) {
        println!("(Run the JGrab daemon to see its version)");
    }
}

fn log(message: &str) {
    println!("=== JGrab Client - {} ===", message)
}

fn usage_error(message: &str) -> ! {
    println!("### {} ###\n\n{}", message, JGRAB_USAGE);
    exit(2)
}

fn error(message: &str) -> ! {
    println!("### JGrab Client Error - {} ###", message);
    exit(1)
}
