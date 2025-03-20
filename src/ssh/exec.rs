use ssh2::Session;
use std::io::Read;
use std::io::{stdin, stdout, Write};
use std::net::TcpStream;

use anyhow::Result;
use async_trait::async_trait;
use russh::keys::{decode_secret_key, PrivateKey, PrivateKeyWithHashAlg};
use russh::{client, ChannelMsg, Disconnect};
use ssh_key;
use std::{str, sync::Arc};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::ToSocketAddrs;
use tracing::{debug, info};

/// A no-op client handler that just accepts the host key without checking.
/// In production, you’d want to verify the server’s public key or do known_hosts logic.
struct ClientHandler;

#[allow(unused_variables)]
#[async_trait::async_trait]
impl client::Handler for ClientHandler {
    type Error = russh::Error;
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
) -> Result<String>
where
    A: ToSocketAddrs + std::fmt::Debug,
{
    debug!(
        "Executing SSH command with host: {:?}, username: {}, private_key: {}, command: {}",
        host, username, private_key, command
    );
    // 1) Decode your in-memory Ed25519 private key
    //    (If your key has a passphrase, pass `Some("pass")` instead of `None`.)
    let parsed_key = decode_secret_key(private_key, None)
        .map_err(|e| anyhow::anyhow!("Failed to parse: {}", e))?;

    debug!("Parsed key: {:?}", parsed_key);

    // 2) Create a minimal client config
    let mut config = client::Config::default();
    // Example: set a small inactivity timeout
    config.inactivity_timeout = Some(std::time::Duration::from_secs(5));
    let config = Arc::new(config);

    debug!("Connecting to SSH server");
    // 3) Connect to the SSH server
    let handler = ClientHandler;
    let mut session = client::connect(config, host, handler)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect SSH: {}", e))?;

    debug!("Connected to SSH server");

    // 4) Authenticate with public key
    //    If you need e.g. "rsa-sha2-256" you can call `session.best_supported_rsa_hash().await?`
    //    For Ed25519, we usually pass `None` for the “best_supported_rsa_hash”:
    let auth_res = session
        .authenticate_publickey(
            username,
            PrivateKeyWithHashAlg::new(Arc::new(parsed_key), None),
        )
        .await?;

    debug!("Authenticated with SSH server");

    if !auth_res.success() {
        return Err(anyhow::anyhow!("SSH authentication failed"));
    }

    // 5) Open a channel and exec your command
    let mut channel = session.channel_open_session().await?;
    // “false” means “no request for a PTY”; set `true` if you want pseudo-tty allocation
    channel.exec(false, command).await?;

    debug!("Executed command");

    // 6) Read the command’s stdout from the channel
    let mut output = String::new();
    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::Data { data } => {
                // Convert bytes to UTF-8 (ignoring invalid sequences).
                if let Ok(text) = str::from_utf8(&data) {
                    debug!("Received data: {}", text);
                    output.push_str(text);
                }
            }
            ChannelMsg::ExitStatus { exit_status } => {
                // If you need the remote exit code, store `exit_status`.
                debug!("Received exit status: {}", exit_status);
            }
            ChannelMsg::ExitSignal { signal_name, .. } => {
                // E.g. the remote was signaled. You can handle if you like.
                debug!("Received exit signal: {:?}", signal_name);
            }
            ChannelMsg::Eof => {
                // The server closed the channel’s sending end
                debug!("Received EOF");
            }
            // Possibly other variants, ignoring them here.
            _ => {}
        }
    }

    // 7) Disconnect politely (optional).
    session
        .disconnect(Disconnect::ByApplication, "", "en-US")
        .await?;

    debug!("Disconnected from SSH server");

    // 8) Return the stdout
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
