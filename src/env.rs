use std::path::PathBuf;
use std::process::Command;

pub struct Env {
    pub std_dir: PathBuf,
}

impl Env {
    pub fn find() -> Option<Self> {
        let Ok(output) = Command::new("zig").arg("env").output() else {
            return None;
        };
        if !output.status.success() {
            return None;
        }
        // TODO: use Zig standard library to parse ZON
        let Ok(stdout) = String::from_utf8(output.stdout) else {
            return None;
        };
        let index = stdout.find(".std_dir =")?;
        let slice = &stdout[index..];
        let offset = slice.find('"')? + 1;
        let length = slice[offset..].find('"')?;
        Some(Self {
            std_dir: PathBuf::from(&slice[offset..offset + length]),
        })
    }
}
