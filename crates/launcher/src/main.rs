use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(msg) => {
            eprintln!("launcher error: {msg}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<u8, String> {
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    if args.is_empty() {
        print_usage();
        return Err("missing arguments".to_string());
    }

    let (env_file, config_file, mut command_parts) = parse_args(args)?;
    load_env_file(&env_file)?;

    if command_parts.is_empty() {
        return Err("missing target command after '--'".to_string());
    }

    if let Some(cfg) = config_file {
        // Pass config to target process only when caller requested it.
        command_parts.push(OsString::from("--config"));
        command_parts.push(OsString::from(cfg));
    }

    let mut cmd = Command::new(&command_parts[0]);
    if command_parts.len() > 1 {
        cmd.args(&command_parts[1..]);
    }

    let status = cmd
        .status()
        .map_err(|e| format!("failed to start command '{}': {e}", command_parts[0].to_string_lossy()))?;

    let code = status.code().unwrap_or(1);
    Ok(code.clamp(0, 255) as u8)
}

fn parse_args(args: Vec<OsString>) -> Result<(String, Option<String>, Vec<OsString>), String> {
    let mut env_file = ".env".to_string();
    let mut config_file: Option<String> = None;
    let mut i = 0usize;
    let mut command_start = None;

    while i < args.len() {
        let token = args[i].to_string_lossy();
        match token.as_ref() {
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            "--env-file" => {
                if i + 1 >= args.len() {
                    return Err("missing value for --env-file".to_string());
                }
                env_file = args[i + 1].to_string_lossy().to_string();
                i += 2;
            }
            "--config" => {
                if i + 1 >= args.len() {
                    return Err("missing value for --config".to_string());
                }
                config_file = Some(args[i + 1].to_string_lossy().to_string());
                i += 2;
            }
            "--" => {
                command_start = Some(i + 1);
                break;
            }
            _ => {
                return Err(format!(
                    "unknown argument '{token}'. expected --env-file, --config or --"
                ));
            }
        }
    }

    let start = command_start.ok_or_else(|| "missing '--' separator".to_string())?;
    let command_parts = args[start..].to_vec();
    Ok((env_file, config_file, command_parts))
}

fn load_env_file(path: &str) -> Result<(), String> {
    let path_ref = Path::new(path);
    let raw = fs::read_to_string(path_ref)
        .map_err(|e| format!("cannot read env file '{}': {e}", path_ref.display()))?;

    for (line_no, raw_line) in raw.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (key, value) = parse_env_line(line)
            .map_err(|e| format!("invalid env line {} in '{}': {}", line_no + 1, path_ref.display(), e))?;

        if key.is_empty() {
            return Err(format!(
                "invalid env line {} in '{}': empty variable name",
                line_no + 1,
                path_ref.display()
            ));
        }

        env::set_var(key, value);
    }

    Ok(())
}

fn parse_env_line(line: &str) -> Result<(&str, String), String> {
    let sep = line
        .find('=')
        .ok_or_else(|| "expected KEY=VALUE".to_string())?;
    let key = line[..sep].trim();
    let raw_value = line[sep + 1..].trim();

    let value = if (raw_value.starts_with('"') && raw_value.ends_with('"'))
        || (raw_value.starts_with('\'') && raw_value.ends_with('\''))
    {
        raw_value[1..raw_value.len() - 1].to_string()
    } else {
        raw_value.to_string()
    };

    Ok((key, value))
}

fn print_usage() {
    eprintln!("Usage:");
    eprintln!("  launcher [--env-file <path-to-env>] [--config <path-to-toml>] -- <program> [args...]");
    eprintln!("Examples:");
    eprintln!("  launcher --env-file .\\secrets\\central.env -- .\\central-server.exe");
    eprintln!("  launcher --env-file .\\secrets\\edge-51.env --config .\\config\\edge-51.toml -- .\\edge-agent.exe");
}

#[cfg(test)]
mod tests {
    use super::parse_env_line;

    #[test]
    fn parse_plain_value() {
        let (k, v) = parse_env_line("A=1").expect("must parse");
        assert_eq!(k, "A");
        assert_eq!(v, "1");
    }

    #[test]
    fn parse_quoted_value() {
        let (k, v) = parse_env_line("A=\"hello world\"").expect("must parse");
        assert_eq!(k, "A");
        assert_eq!(v, "hello world");
    }
}
