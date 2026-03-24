#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatModeArg {
    Off,
    One,
    All,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    List,
    Queue,
    Find(String),
    QueueFind(String),
    Search(String),
    Resolve(String),
    PlayUrl(String),
    Play(usize),
    PlayName(String),
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
    Snapshot,
    Reload,
    Help,
    Exit,
    Unknown(String),
}

impl Command {
    pub fn parse(input: &str) -> Self {
        let mut parts = input.split_whitespace();
        let Some(cmd) = parts.next() else {
            return Command::Help;
        };

        match cmd {
            "list" => Command::List,
            "queue" => Command::Queue,
            "find" => Command::Find(parts.collect::<Vec<_>>().join(" ")),
            "queuefind" => Command::QueueFind(parts.collect::<Vec<_>>().join(" ")),
            "search" => Command::Search(parts.collect::<Vec<_>>().join(" ")),
            "resolve" => Command::Resolve(parts.collect::<Vec<_>>().join(" ")),
            "playurl" => Command::PlayUrl(parts.collect::<Vec<_>>().join(" ")),
            "play" => match parts.next().and_then(|s| s.parse::<usize>().ok()) {
                Some(index) => Command::Play(index),
                None => Command::Unknown(input.to_string()),
            },
            "playname" => Command::PlayName(parts.collect::<Vec<_>>().join(" ")),
            "next" => Command::Next,
            "prev" => Command::Prev,
            "pause" => Command::Pause,
            "resume" => Command::Resume,
            "stop" => Command::Stop,
            "volume" => match parts.next().and_then(|s| s.parse::<f32>().ok()) {
                Some(v) => Command::Volume(v),
                None => Command::Unknown(input.to_string()),
            },
            "seek" => match parts.next().and_then(|s| s.parse::<u64>().ok()) {
                Some(sec) => Command::Seek(sec),
                None => Command::Unknown(input.to_string()),
            },
            "pos" => Command::Pos,
            "repeat" => match parts.next() {
                Some("off") => Command::Repeat(RepeatModeArg::Off),
                Some("one") => Command::Repeat(RepeatModeArg::One),
                Some("all") => Command::Repeat(RepeatModeArg::All),
                _ => Command::Unknown(input.to_string()),
            },
            "shuffle" => match parts.next() {
                Some("on") => Command::Shuffle(true),
                Some("off") => Command::Shuffle(false),
                _ => Command::Unknown(input.to_string()),
            },
            "status" => Command::Status,
            "snapshot" => Command::Snapshot,
            "reload" => Command::Reload,
            "help" => Command::Help,
            "exit" | "quit" => Command::Exit,
            _ => Command::Unknown(input.to_string()),
        }
    }
}
