use ssh2::Session;
use std::io::Read;
use std::io::{stdin, stdout, Write};
use std::net::TcpStream;

/// Executes a command over SSH using an in-memory Ed25519 private key.
/// Returns the command's stdout on success.
pub fn exec_ssh_command(
    host: &str,
    username: &str,
    private_key: &str,
    command: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // 1) Open a TCP connection to the remote SSH port
    let tcp = TcpStream::connect((host, 22))?;

    // 2) Create an SSH session
    let mut sess = Session::new()?;
    sess.set_tcp_stream(tcp);
    sess.handshake()?;

    // 3) Authenticate using the Ed25519 private key (in OpenSSH format)
    //    If your key doesn't have a passphrase, specify None here
    sess.userauth_pubkey_memory(username, None, &private_key, None)?;
    if !sess.authenticated() {
        return Err("SSH authentication failed".into());
    }

    // 4) Open a channel and run the command
    let mut channel = sess.channel_session()?;
    channel.exec(command)?;

    // 5) Read the command's stdout into a string
    let mut output = String::new();
    channel.read_to_string(&mut output)?;

    // 6) Cleanup
    channel.wait_close()?;
    let _exit_status = channel.exit_status()?; // If needed, handle non-zero exit

    Ok(output)
}

/// Opens an interactive SSH shell (like an 'ssh' CLI).
pub fn open_ssh_shell(
    host: &str,
    username: &str,
    private_key: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 1) Connect TCP to the remote host on port 22
    let tcp = TcpStream::connect((host, 22))?;

    // 2) Create an SSH session
    let mut session = Session::new()?;
    session.set_tcp_stream(tcp);
    session.handshake()?;

    // 3) Authenticate with the in-memory Ed25519 key
    session.userauth_pubkey_memory(username, None, private_key, None)?;
    if !session.authenticated() {
        return Err("SSH authentication failed".into());
    }

    // 4) Open a session channel with a PTY
    let mut channel = session.channel_session()?;
    channel.request_pty("xterm", None, None)?; // "xterm" is just an example

    // 5) Start a shell
    channel.shell()?;

    // 6) Bridge standard input/output to/from the channel
    //    This is a very minimal example: you might want a more sophisticated
    //    approach using non-blocking I/O or separate threads.
    let mut stdin_clone = stdin();
    let mut stdout_clone = stdout();

    // Read from the channel (remote -> local stdout)
    let mut remote_out = channel.stream(0);
    let mut buf = [0u8; 1024];

    loop {
        // Forward data from remote (SSH) to local (stdout)
        if let Ok(size) = remote_out.read(&mut buf) {
            if size == 0 {
                break;
            }
            stdout_clone.write_all(&buf[..size])?;
            stdout_clone.flush()?;
        }

        // Forward data from local (stdin) to remote (SSH)
        if let Ok(size) = stdin_clone.read(&mut buf) {
            if size > 0 {
                channel.write_all(&buf[..size])?;
            }
        }

        // You might also want to check for channel's exit status or `eof` here.
        // break if `channel.eof()` or user types `exit`
    }

    // Wait for the shell to close
    channel.wait_close()?;
    Ok(())
}
