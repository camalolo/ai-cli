use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::io::{self, Read, Write};
use std::process::Command;
use std::str;
use std::thread;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::fs::File;
use std::os::fd::{AsRawFd, FromRawFd};
use pty::fork::*;

use crate::sandbox::get_sandbox_root;

pub fn execute_command(command: &str) -> String {
     // Enable raw mode to hide passwords during command execution
     enable_raw_mode().expect("Failed to enable raw mode");

     if command.trim().is_empty() {
         disable_raw_mode().expect("Failed to disable raw mode");
         return "Error: No command provided".to_string();
     }

     let (program, args) = get_command_parts(command);

    let fork = Fork::from_ptmx().expect("Failed to create PTY");
    match fork {
        Fork::Parent(pid, master) => {
            let stop = Arc::new(AtomicBool::new(false));
            let stop_clone = Arc::clone(&stop);

             let duped_fd = unsafe { libc::dup(master.as_raw_fd()) };
             let master_clone = unsafe { File::from_raw_fd(duped_fd) };
             let input_handle = thread::spawn(move || {
                 let mut master = master_clone;
                 let mut buffer = [0u8; 1024];

                 // Dup stdin fd
                 let stdin_fd = io::stdin().as_raw_fd();
                 let dup_stdin_fd = unsafe { libc::dup(stdin_fd) };
                 let mut stdin_file = unsafe { File::from_raw_fd(dup_stdin_fd) };

                 loop {
                     if stop_clone.load(Ordering::Relaxed) {
                         break;
                     }
                     let mut fds = [libc::pollfd { fd: dup_stdin_fd, events: libc::POLLIN, revents: 0 }];
                     let ret = unsafe { libc::poll(fds.as_mut_ptr(), 1, 0) };
                     if ret > 0 && (fds[0].revents & libc::POLLIN) != 0 {
                         match stdin_file.read(&mut buffer) {
                             Ok(0) => break,
                             Ok(n) => {
                                 if master.write_all(&buffer[..n]).is_err() {
                                     break;
                                 }
                             }
                             Err(_) => break,
                         }
                     } else if ret < 0 {
                         break;
                     }
                     // No data available, continue polling
                 }
             });

            let (tx, rx) = std::sync::mpsc::channel();
            let duped_fd2 = unsafe { libc::dup(master.as_raw_fd()) };
            let master_clone2 = unsafe { File::from_raw_fd(duped_fd2) };
             let output_handle = thread::spawn(move || {
                 let mut master = master_clone2;
                 let mut output = Vec::new();
                 let mut buffer = [0u8; 1024];
                 loop {
                     match master.read(&mut buffer) {
                         Ok(0) => break,
                         Ok(n) => {
                             io::stdout().write_all(&buffer[..n]).ok();
                             io::stdout().flush().ok();
                             output.extend_from_slice(&buffer[..n]);
                         }
                         Err(_) => break,
                     }
                 }
                 tx.send(output).ok();
             });

            let mut status: libc::c_int = 0;
            unsafe { libc::waitpid(pid as libc::pid_t, &mut status, 0); }
            stop.store(true, Ordering::Relaxed);
            input_handle.join().ok();
            output_handle.join().ok();

             let output_buf = rx.recv().unwrap_or_default();
             let output_str = String::from_utf8_lossy(&output_buf);
              disable_raw_mode().expect("Failed to disable raw mode");
              if libc::WIFEXITED(status) {
                 let code = libc::WEXITSTATUS(status);
                 if code == 0 {
                     if output_str.is_empty() {
                         "Command executed (no output)".to_string()
                     } else {
                         output_str.to_string()
                     }
                 } else {
                     format!("Command '{}' exited with code {}", command, code)
                 }
              } else {
                  format!("Command '{}' exited abnormally", command)
              }
        }
        Fork::Child(ref slave) => {
            // Set PTY slave to raw mode to disable echo and hide passwords for commands like su
            let fd = slave.as_raw_fd();
            let mut term: libc::termios = unsafe { std::mem::zeroed() };
            unsafe {
                libc::tcgetattr(fd, &mut term);
                libc::cfmakeraw(&mut term);
                libc::tcsetattr(fd, libc::TCSANOW, &term);
            }
            let _ = Command::new(&program)
                .args(&args)
                .current_dir(get_sandbox_root())
                .status()
                .expect("Failed to execute command");
            std::process::exit(0);
        }
    }
}

fn get_command_parts(command: &str) -> (String, Vec<String>) {
    #[cfg(target_os = "linux")]
    {
        // Use bwrap
        let args = vec![
            "--ro-bind".to_string(), "/".to_string(), "/".to_string(),
            "--bind".to_string(), get_sandbox_root().clone(), get_sandbox_root().clone(),
            "--dev".to_string(), "/dev".to_string(),
            "--proc".to_string(), "/proc".to_string(),
            "/bin/sh".to_string(), "-c".to_string(), command.to_string(),
        ];
        ("bwrap".to_string(), args)
    }

    #[cfg(target_os = "windows")]
    {
        // Use cmd
        ("cmd".to_string(), vec!["/C".to_string(), command.to_string()])
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        // Use shell
        let shell = if cfg!(target_os = "macos") { "/bin/zsh" } else { "/bin/sh" };
        (shell.to_string(), vec!["-c".to_string(), command.to_string()])
    }
}

