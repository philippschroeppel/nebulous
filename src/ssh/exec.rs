use std::io::Read;
use std::io::{stdin, stdout, Write};
use std::io::{Error as IoError, ErrorKind};
use std::net::TcpStream;

use anyhow::Result;
use russh::keys::{decode_secret_key, PrivateKeyWithHashAlg};
use russh::{client, ChannelMsg, Disconnect};
use std::{str, sync::Arc};
use tracing::debug;

use russh::client::Config;
use russh::keys::*;
use russh::*;
use ssh2::Session;
use std::process::Command;
use tokio::io::AsyncWriteExt;
use tokio::net::ToSocketAddrs;

/// Execute a command on a container (accessible via Tailscale SSH).
///
/// # Arguments
///
/// * `namespace` - The container's namespace
/// * `name` - The container's name
/// * `opts` - SSH options, including interactive, tty, and the command itself
///
/// # Example
///
/// ```rust
/// let opts = ExecSshOptions {
///     interactive: true,
///     tty: true,
///     command: vec!["ls".to_string(), "-lah".to_string()],
/// };
/// if let Err(e) = run_ssh_command("default", "mycontainer", &opts) {
///     eprintln!("Failed to exec via SSH: {:?}", e);
/// }
/// ```
pub fn run_ssh_command_ts(
    hostname: &str,
    command: Vec<String>,
    interactive: bool,
    tty: bool,
    username: Option<&str>,
) -> Result<String, IoError> {
    debug!(
        "Running SSH command: '{:?}' on {hostname} as {:?} with interactive={interactive} and tty={tty}",
        command, username
    );

    let mut ssh_cmd = Command::new("ssh");

    // Disable host key checking and skip writing to known_hosts:
    ssh_cmd.arg("-o").arg("StrictHostKeyChecking=no");
    ssh_cmd.arg("-o").arg("UserKnownHostsFile=/dev/null");

    if let Some(u) = username {
        // Option A: "ssh user@host"
        ssh_cmd.arg(format!("{u}@{hostname}"));
        // or Option B: "ssh -l user host"
        //   ssh_cmd.arg("-l").arg(u).arg(hostname);
    } else {
        ssh_cmd.arg(hostname);
    }

    if interactive {
        ssh_cmd.arg("-i");
    }

    if tty {
        ssh_cmd.arg("-t");
    }

    // Append the command to be run
    ssh_cmd.args(command);

    // Capture output
    let output = ssh_cmd
        .output()
        .map_err(|err| IoError::new(ErrorKind::Other, format!("Failed to spawn ssh: {err}")))?;

    if !output.status.success() {
        return Err(IoError::new(
            ErrorKind::Other,
            format!(
                "SSH command failed with status: {:?}\nStderr:\n{}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr),
            ),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

struct Client {}

// More SSH event handlers
// can be defined in this trait
// In this example, we're only using Channel, so these aren't needed.
impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}
/// Asynchronous function to execute a command via SSH using `russh`.
///
/// - `host` should be something like "example.com:22" (or ip:port).
/// - `username` is your SSH username.
/// - `private_key` is the **in-memory** Ed25519 private key in OpenSSH format.
/// - `command` is the remote command to be executed.
///
/// Returns the command's stdout as a `String`.
pub async fn exec_ssh_command<A: ToSocketAddrs>(
    host: A,
    username: &str,
    private_key: &str,
    command: &str,
) -> anyhow::Result<String>
where
    A: ToSocketAddrs + std::fmt::Debug,
{
    let mut config = Config::default();
    let config = Arc::new(config);

    // 1) Connect
    debug!("Connecting to SSH server");
    let handler = Client {};
    let mut session = client::connect(config, host, handler).await?;

    // 2) Decode your in-memory private key
    //
    //    Note: If your key is encrypted (passphrase-protected),
    //    you’d need to pass `Some("passphrase")` as the second arg.
    let raw_key = russh::keys::decode_secret_key(private_key, None)?;

    // 3) Authenticate with the remote server
    let key_with_hash = PrivateKeyWithHashAlg::new(Arc::new(raw_key), Some(HashAlg::Sha256));
    match session
        .authenticate_publickey(username, key_with_hash)
        .await?
    {
        russh::client::AuthResult::Success => {
            // e.g. log success
            debug!("Authenticated with SSH server");
        }
        russh::client::AuthResult::Failure {
            remaining_methods,
            partial_success,
        } => {
            // e.g. return an error
            return Err(anyhow::anyhow!(
                "Public-key authentication failed: partial_success={partial_success:?}",
            ));
        }
    }

    // 4) Open a channel
    let mut channel = session.channel_open_session().await?;

    // 1) Request a PTY + Shell
    channel
        .request_pty(true, "xterm", 80, 24, 0, 0, &[])
        .await?;
    channel.request_shell(true).await?;

    // 2) Send our command, then a special marker, then exit.
    //    We’ll look for CMD_DONE_... to know when we're done.
    let marker = "CMD_DONE_1234"; // Could be a random UUID for uniqueness
    let script = format!(
        "{cmd}\necho {marker}\nexit\n",
        cmd = command,
        marker = marker
    );

    let mut writer = channel.make_writer();
    writer.write_all(script.as_bytes()).await?;
    channel.eof().await?;

    // 3) Read until we see our marker. We'll gather only
    //    the lines that come AFTER the echoed command line
    //    but BEFORE the marker line.
    let mut output = String::new();

    // We'll do line buffering in case Data arrives in fragments
    let mut buffer = String::new();
    let mut collecting = false; // are we in the region after the echoed command?

    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::Data { data } => {
                if let Ok(text) = std::str::from_utf8(&data) {
                    buffer.push_str(text);

                    // Process lines that might accumulate in `buffer`
                    while let Some(newline_idx) = buffer.find('\n') {
                        // Extract everything up to newline_idx
                        let raw_line = buffer[..newline_idx].to_string();
                        let line = raw_line.replace('\r', "");
                        debug!("Line: '{}'", line.replace('\n', "\\n"));

                        // Remove up to and including the newline character
                        buffer.drain(..=newline_idx);

                        // Now we decide if we record this line, or skip it
                        if line.contains(command) {
                            // We found the line that echoed back our command.
                            // Start collecting on subsequent lines.
                            collecting = true;
                            continue;
                        }
                        if line.contains(marker) {
                            // Marker means we're done collecting the command's output
                            collecting = false;
                            continue;
                        }
                        if collecting {
                            // This line is "pure output" from the command
                            output.push_str(&line);
                            output.push('\n');
                        }
                    }
                }
            }
            ChannelMsg::Eof | ChannelMsg::Close => {
                // The remote shell is closed or done
                break;
            }
            _ => {}
        }
    }

    // 4) Close channel politely (optional if everything ended anyway)
    channel.close().await?;

    debug!("Output: {}", output);
    Ok(output)
}

/// Asynchronous function to execute a command via SSH using [`async_ssh2_tokio`].
///
/// - `host` should be just the hostname or IP (e.g. "example.com" or "10.10.10.2").  
///   We’ll connect on port 22 below.  
/// - `username` is your SSH username.  
/// - `private_key` is the **in-memory** Ed25519 (or RSA, etc.) private key contents
///   in OpenSSH format. If you have a passphrase, use the [`AuthMethod::with_key`]
///   variant with `Some("your-passphrase")`.
/// - `command` is the remote command to be executed.
///
/// Returns the command's stdout as a `String`.
pub async fn exec_ssh_command_async_ssh2tokio(
    host: &str,
    username: &str,
    private_key: &str,
    command: &str,
) -> Result<String, async_ssh2_tokio::Error> {
    use async_ssh2_tokio::client::{AuthMethod, Client, ServerCheckMethod};

    // 1) Configure our authentication method. Here, we assume an in-memory private key.
    //    If your private key has a passphrase, replace `None` with `Some("passphrase")`.
    let auth_method = AuthMethod::with_key(private_key, None);

    // 2) Connect to the SSH server
    //
    //    For demonstration, we do a "no-check" for the server host key.
    //    In production, you'd generally want to do some key/host-fingerprint checks
    //    to ensure you're really talking to the correct server.
    let mut client = Client::connect(
        (host, 22), // host, port
        username,
        auth_method,
        ServerCheckMethod::NoCheck,
    )
    .await?;

    // 3) Execute the remote command
    let result = client.execute(command).await?;

    // 4) Finally, disconnect if desired (optional, but recommended).
    //    If not explicitly called, the session/connection will drop when `client` is out of scope.
    client.disconnect().await?;

    // 5) Return the standard output (stderr is separate in `result.stderr`).
    Ok(result.stdout)
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
