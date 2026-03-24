mod app;
mod command;
mod player;
mod music;
mod queue;
mod snapshot;

use anyhow::Result;
use crate::app::App;

fn main() -> Result<()> {
    let mut app = App::bootstrap()?;
    app.run()
}