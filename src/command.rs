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
    Open(String),
    PlayUrl(String),
    Play(usize),
    PlayName(String),
    Contract,
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
        let parts = shell_words::split(input).unwrap_or_else(|_| {
            input
                .split_whitespace()
                .map(|part| part.to_string())
                .collect()
        });
        Self::parse_parts(parts)
    }

    pub fn parse_parts(parts: Vec<String>) -> Self {
        Self::parse_parts_impl(parts)
    }

    fn parse_parts_impl(parts: Vec<String>) -> Self {
        let mut parts = parts.into_iter();
        let Some(cmd) = parts.next() else {
            return Command::Help;
        };

        let rest = parts.collect::<Vec<_>>();
        let input = std::iter::once(cmd.clone())
            .chain(rest.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ");

        match cmd.as_str() {
            "list" | "ls" => Command::List,
            "queue" | "q" => Command::Queue,
            "find" => Command::Find(rest.join(" ")),
            "queuefind" => Command::QueueFind(rest.join(" ")),
            "search" => Command::Search(rest.join(" ")),
            "resolve" => Command::Resolve(rest.join(" ")),
            "open" => Command::Open(rest.join(" ")),
            "playurl" => Command::PlayUrl(rest.join(" ")),
            "play" => {
                let value = rest.join(" ");
                if value.is_empty() {
                    Command::Unknown(input)
                } else if let Ok(index) = value.parse::<usize>() {
                    Command::Play(index)
                } else {
                    Command::PlayName(value)
                }
            }
            "playname" => Command::PlayName(rest.join(" ")),
            "contract" => Command::Contract,
            "next" => Command::Next,
            "prev" => Command::Prev,
            "pause" => Command::Pause,
            "resume" => Command::Resume,
            "stop" => Command::Stop,
            "volume" => match rest.first().and_then(|s| s.parse::<f32>().ok()) {
                Some(v) => Command::Volume(v),
                None => Command::Unknown(input),
            },
            "seek" => match rest.first().and_then(|s| s.parse::<u64>().ok()) {
                Some(sec) => Command::Seek(sec),
                None => Command::Unknown(input),
            },
            "pos" => Command::Pos,
            "repeat" => match rest.first().map(|s| s.as_str()) {
                Some("off") => Command::Repeat(RepeatModeArg::Off),
                Some("one") => Command::Repeat(RepeatModeArg::One),
                Some("all") => Command::Repeat(RepeatModeArg::All),
                _ => Command::Unknown(input),
            },
            "shuffle" => match rest.first().map(|s| s.as_str()) {
                Some("on") => Command::Shuffle(true),
                Some("off") => Command::Shuffle(false),
                _ => Command::Unknown(input),
            },
            "status" => Command::Status,
            "snapshot" => Command::Snapshot,
            "reload" => Command::Reload,
            "help" => Command::Help,
            "exit" | "quit" => Command::Exit,
            _ => Command::Unknown(input),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Command, RepeatModeArg};

    #[test]
    fn parses_open_command_with_quoted_path() {
        let command = Command::parse(r#"open "C:/Music/Best Track.mp3""#);

        assert_eq!(
            command,
            Command::Open(r#"C:/Music/Best Track.mp3"#.to_string())
        );
    }

    #[test]
    fn play_accepts_text_query_fallback() {
        let command = Command::parse("play lo-fi mix");

        assert_eq!(command, Command::PlayName("lo-fi mix".to_string()));
    }

    #[test]
    fn parses_repeat_mode() {
        let command = Command::parse("repeat all");

        assert_eq!(command, Command::Repeat(RepeatModeArg::All));
    }
}
