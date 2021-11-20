use std::env;
use telexide::{api::types::*, prelude::*};

use log::{error, LevelFilter};
use simple_logger::SimpleLogger;

use tokio::time::{sleep, Duration};

use raf::persistence::db::connection;
use raf::persistence::types::*;

use raf::telegram::commands::*;
use raf::telegram::handlers;

#[tokio::main]
async fn main() {
    SimpleLogger::new()
        .with_level(LevelFilter::Info)
        .init()
        .unwrap();

    let pool = connection();
    let token = env::var("TOKEN").expect("Provide the token via TOKEN env var");
    let bot_name = env::var("BOT_NAME").expect("Provide the bot name via BOT_NAME env var");

    let client = ClientBuilder::new()
        .set_token(&token)
        .set_framework(create_framework!(
            &bot_name, help, start, register, contest, list, rank
        ))
        .set_allowed_updates(vec![UpdateType::CallbackQuery, UpdateType::Message])
        .add_handler_func(handlers::message)
        .add_handler_func(handlers::callback)
        .build();

    {
        let mut data = client.data.write();
        data.insert::<DBKey>(pool);
        data.insert::<NameKey>(bot_name);
    }

    loop {
        let ret = client.start().await;
        match ret {
            Err(err) => {
                error!("ApiResponse {}\nWaiting a minute and retrying...", err);
                sleep(Duration::from_secs(60)).await;
            }
            Ok(()) => {
                error!("Exiting from main loop without an error, but this should never happen!");
                break;
            }
        }
    }
}
