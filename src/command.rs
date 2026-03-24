use std::str::FromStr;

#[derive(Debug, Clone, Copy)]
pub enum RepeatModeArg {
    Off,
    One,
    All,
}

#[derive(Debug)]
pub enum Command {
    List,
    Play(usize),
    Next,
    Prev,
    Pause,
    Resume,
    Stop,
    Volume(f32),
    Seek(u64),
    Pos,
    Repeat(RepeatModeArg),
    Shuffle(bool),
    Status,
    Reload,
    Help,
    Exit,
    Unknown(String),
}

impl Command {
    pub fn parse(input: &str) -> Self {
        let mut parts = input.split_whitespace();
        let cmd = parts.next().unwrap_or("");

        match cmd {
            "list" => Command::List,
            "play" => {
                let Some(raw_index) = parts.next() else {
                    return Command::Unknown("play".to_string());
                };

                match usize::from_str(raw_index) {
                    Ok(index) => Command::Play(index),
                    Err(_) => Command::Unknown("play".to_string()),
                }
            }
            "next" => Command::Next,
            "prev" => Command::Prev,
            "pause" => Command::Pause,
            "resume" => Command::Resume,
            "stop" => Command::Stop,
            "volume" => {
                let Some(raw_volume) = parts.next() else {
                    return Command::Unknown("volume".to_string());
                };

                match f32::from_str(raw_volume) {
                    Ok(volume) => Command::Volume(volume),
                    Err(_) => Command::Unknown("volume".to_string()),
                }
            }
            "seek" => {
                let Some(raw_secs) = parts.next() else {
                    return Command::Unknown("seek".to_string());
                };

                match u64::from_str(raw_secs) {
                    Ok(secs) => Command::Seek(secs),
                    Err(_) => Command::Unknown("seek".to_string()),
                }
            }
            "pos" => Command::Pos,
            "repeat" => {
                let Some(raw_mode) = parts.next() else {
                    return Command::Unknown("repeat".to_string());
                };

                match raw_mode {
                    "off" => Command::Repeat(RepeatModeArg::Off),
                    "one" => Command::Repeat(RepeatModeArg::One),
                    "all" => Command::Repeat(RepeatModeArg::All),
                    _ => Command::Unknown("repeat".to_string()),
                }
            }
            "shuffle" => {
                let Some(raw_value) = parts.next() else {
                    return Command::Unknown("shuffle".to_string());
                };

                match raw_value {
                    "on" => Command::Shuffle(true),
                    "off" => Command::Shuffle(false),
                    _ => Command::Unknown("shuffle".to_string()),
                }
            }
            "status" => Command::Status,
            "reload" => Command::Reload,
            "help" => Command::Help,
            "exit" => Command::Exit,
            other => Command::Unknown(other.to_string()),
        }
    }
}
