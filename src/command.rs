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
    Providers,
    ProviderSet {
        provider_id: String,
        payload: String,
    },
    ProviderClear(String),
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
        if let Some(command) = Self::parse_provider_command(input) {
            return command;
        }

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

    fn parse_provider_command(input: &str) -> Option<Self> {
        let trimmed = input.trim();
        let rest = trimmed.strip_prefix("provider")?;

        if rest.is_empty() {
            return Some(Command::Providers);
        }

        let rest = rest.trim_start();
        if rest.is_empty() {
            return Some(Command::Providers);
        }

        let (subcommand, remaining) = split_once_whitespace(rest);
        match subcommand {
            "list" | "ls" | "status" => Some(Command::Providers),
            "clear" => {
                let (provider_id, _) = split_once_whitespace(remaining);
                if provider_id.is_empty() {
                    Some(Command::Unknown(trimmed.to_string()))
                } else {
                    Some(Command::ProviderClear(provider_id.to_string()))
                }
            }
            "set" => {
                let (provider_id, payload) = split_once_whitespace(remaining);
                if provider_id.is_empty() || payload.trim().is_empty() {
                    Some(Command::Unknown(trimmed.to_string()))
                } else {
                    Some(Command::ProviderSet {
                        provider_id: provider_id.to_string(),
                        payload: payload.trim_start().to_string(),
                    })
                }
            }
            _ => None,
        }
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
            "providers" => Command::Providers,
            "provider" => match rest.first().map(|part| part.as_str()) {
                Some("list") | Some("ls") | Some("status") | None => Command::Providers,
                Some("set") if rest.len() >= 3 => Command::ProviderSet {
                    provider_id: rest[1].clone(),
                    payload: rest[2..].join(" "),
                },
                Some("clear") if rest.len() >= 2 => Command::ProviderClear(rest[1].clone()),
                _ => Command::Unknown(input),
            },
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

fn split_once_whitespace(input: &str) -> (&str, &str) {
    let input = input.trim_start();
    let Some(index) = input.find(char::is_whitespace) else {
        return (input, "");
    };

    let (head, tail) = input.split_at(index);
    (head, tail.trim_start())
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

    #[test]
    fn parses_provider_set_json_payload() {
        let command =
            Command::parse(r#"provider set spotify {"enabled":true,"access_token":"abc123"}"#);

        assert_eq!(
            command,
            Command::ProviderSet {
                provider_id: "spotify".to_string(),
                payload: r#"{"enabled":true,"access_token":"abc123"}"#.to_string(),
            }
        );
    }
}
