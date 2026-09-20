use std::collections::BTreeMap;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use wait_timeout::ChildExt;

use crate::stage::GateError;

#[derive(Debug, Clone)]
pub struct ProcessSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: BTreeMap<String, String>,
    pub timeout: Duration,
    pub max_output_bytes: usize,
    pub allow_network: bool,
}

impl ProcessSpec {
    pub fn new(program: impl Into<PathBuf>, cwd: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: cwd.into(),
            env: BTreeMap::new(),
            timeout: Duration::from_secs(300),
            max_output_bytes: 1_048_576,
            allow_network: false,
        }
    }

    pub fn args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn max_output_bytes(mut self, max: usize) -> Self {
        self.max_output_bytes = max;
        self
    }

    pub fn allow_network(mut self, allow: bool) -> Self {
        self.allow_network = allow;
        self
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProcessOutput {
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

impl ProcessOutput {
    pub fn success(&self) -> bool {
        self.status == Some(0)
    }

    pub fn combined(&self) -> String {
        if self.stderr.trim().is_empty() {
            self.stdout.clone()
        } else if self.stdout.trim().is_empty() {
            self.stderr.clone()
        } else {
            format!("{}\n{}", self.stdout, self.stderr)
        }
    }
}

pub fn run(spec: &ProcessSpec) -> Result<ProcessOutput, GateError> {
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .current_dir(&spec.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    command.env_clear();
    for key in [
        "PATH",
        "HOME",
        "TMPDIR",
        "TMP",
        "TEMP",
        "LANG",
        "LC_ALL",
        "NODE_OPTIONS",
        "NPM_CONFIG_CACHE",
        "XDG_CACHE_HOME",
    ] {
        if let Ok(value) = std::env::var(key) {
            command.env(key, value);
        }
    }
    command
        .env("TERM", "dumb")
        .env("NO_COLOR", "1")
        .env("CI", "1");
    if !spec.allow_network {
        for key in ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy"] {
            command.env(key, "http://127.0.0.1:9");
        }
        command.env("NO_PROXY", "");
    }
    for (key, value) in &spec.env {
        command.env(key, value);
    }

    let mut child = command.spawn().map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            GateError::ToolMissing {
                tool: spec.program.display().to_string(),
            }
        } else {
            GateError::Io(source)
        }
    })?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let cap = spec.max_output_bytes;

    let stdout_handle = stdout.map(|stream| std::thread::spawn(move || read_capped(stream, cap)));
    let stderr_handle = stderr.map(|stream| std::thread::spawn(move || read_capped(stream, cap)));

    let status = child.wait_timeout(spec.timeout).map_err(GateError::Io)?;
    let timed_out = status.is_none();
    if timed_out {
        let _ = child.kill();
        let _ = child.wait();
    }
    let code = status.and_then(|status| status.code());

    let stdout = stdout_handle
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    let stderr = stderr_handle
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();

    Ok(ProcessOutput {
        status: code,
        stdout,
        stderr,
        timed_out,
    })
}

fn read_capped(mut stream: impl Read, cap: usize) -> String {
    let mut collected: Vec<u8> = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                if collected.len() < cap {
                    let remaining = cap - collected.len();
                    let take = read.min(remaining);
                    collected.extend_from_slice(&buffer[..take]);
                }
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&collected).into_owned()
}
