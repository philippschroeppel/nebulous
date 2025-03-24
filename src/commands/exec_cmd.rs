use nebulous::ssh::exec::run_ssh_command_ts;
use std::error::Error as StdError;

pub async fn exec_cmd(args: crate::cli::ExecArgs) -> Result<(), Box<dyn StdError>> {
    match run_ssh_command_ts(
        &args.namespace,
        &args.name,
        args.command
            .split_whitespace()
            .map(|s| s.to_string())
            .collect(),
        args.interactive,
        args.tty,
    ) {
        Ok(_) => Ok(()),
        Err(e) => Err(e.into()),
    }
}
