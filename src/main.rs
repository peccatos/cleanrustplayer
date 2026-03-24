mod app;
mod command;
mod music;
mod player;
mod provider;
mod queue;
mod search;
mod snapshot;

use crate::app::App;
use anyhow::Result;

fn main() -> Result<()> {
    let mut app = App::bootstrap()?;
    app.run()
}
