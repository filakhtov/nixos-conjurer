use ipc_channel::ipc::{channel, IpcReceiver, IpcSender};
use nix::{
    sys::{
        signal::{kill, Signal},
        wait::{waitpid, WaitPidFlag, WaitStatus},
    },
    unistd::{fork, getpid, ForkResult, Pid},
};
use serde::{Deserialize, Serialize};
use std::{
    fmt::Display,
    process::{exit, Command, Output},
};

macro_rules! error {
    ($($msg:expr),+) => {
        Error { msg: format!($($msg),+) }
    };
}

macro_rules! err {
    ($($msg:expr),+) => {
        Err(error!($($msg),+))
    };
}

type ProcResult<T> = core::result::Result<T, Error>;

#[derive(Serialize, Deserialize, Debug)]
pub struct Error {
    msg: String,
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}

impl std::error::Error for Error {}

pub fn run_forked<T: Serialize + for<'de> Deserialize<'de>, F: Fn() -> T>(f: F) -> ProcResult<T> {
    let (tx, rx) = match channel() {
        Ok(c) => c,
        Err(e) => {
            return err!(
                "failed to create a channel to communicate with the child process: {}",
                e
            )
        }
    };

    match unsafe { fork() } {
        Ok(ForkResult::Parent { child }) => wait_for_child(rx, child),
        Ok(ForkResult::Child) => run_child(tx, f),
        Err(e) => err!("failed to fork a child process: {}", e),
    }
}

fn run_child<T: Serialize, F: Fn() -> T>(tx: IpcSender<ProcResult<T>>, f: F) -> ! {
    if let Err(_) = tx.send(Ok(f())) {
        exit(1);
    }

    exit(0);
}

fn wait_for_child<T: for<'de> Deserialize<'de> + Serialize>(
    rx: IpcReceiver<ProcResult<T>>,
    child_pid: Pid,
) -> ProcResult<T> {
    loop {
        match waitpid(child_pid, Some(WaitPidFlag::WUNTRACED)) {
            Ok(WaitStatus::Exited(_, 0)) => match read_child_status(rx) {
                r => return r,
            },
            Ok(WaitStatus::Signaled(child, Signal::SIGSTOP, _)) => {
                let _ = kill(getpid(), Signal::SIGSTOP);
                let _ = kill(child, Signal::SIGCONT);
            }
            Ok(WaitStatus::Signaled(_, signal, _)) => {
                let pid = getpid();
                if let Err(e) = kill(pid, signal) {
                    return err!("failed to send the {} signal to PID {}: {}", signal, pid, e);
                }
            }
            Ok(WaitStatus::Exited(pid, status)) => {
                if let Err(s) = read_child_status(rx) {
                    return err!(
                        "child process `{}` returned non-zero status {}: {}",
                        pid,
                        status,
                        s.msg
                    );
                }

                return err!(
                    "child process `{}` returned non-zero status {}",
                    pid,
                    status
                );
            }
            Ok(what) => {
                return err!("unexpected wait event happend: {:?}", what);
            }
            Err(e) => {
                return err!("failed to wait for child process to complete: {}", e);
            }
        }
    }
}

fn read_child_status<T: for<'de> Deserialize<'de> + Serialize>(
    rx: IpcReceiver<ProcResult<T>>,
) -> ProcResult<T> {
    match rx.try_recv() {
        Ok(s) => s,
        Err(e) => err!("unable to read the child process result: {}", e),
    }
}

pub fn run_command<C: AsRef<str>>(command: C, args: &[&str]) -> ProcResult<Output> {
    match Command::new(command.as_ref())
        .args(args)
        .env(
            "PATH",
            "/nix/var/nix/profiles/default/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        )
        .env("TMPDIR", "/tmp")
        .env("HOME", "/root")
        .output()
    {
        Ok(o) => Ok(o),
        Err(e) => err!(
            "failed to execute the `{}` command: {}",
            command.as_ref(),
            e
        ),
    }
}

pub fn run_command_checked<C: AsRef<str>>(command: C, args: &[&str]) -> ProcResult<Output> {
    let result = match run_command(&command, args) {
        Ok(o) => o,
        Err(e) => {
            return err!(
                "failed to execute the `{}` command: {}",
                command.as_ref(),
                e
            )
        }
    };

    if result.status.success() {
        return Ok(result);
    }

    err!(
        "the `{}` command returned non-zero status:\n{:?}",
        command.as_ref(),
        result
    )
}
