// Copyright 2021 Paolo Galeone <nessuno@nerdz.eu>
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//    http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::env;
use telexide_fork::{api::types::*, prelude::*};

use log::{error, LevelFilter};
use simple_logger::SimpleLogger;

use tokio::time::{sleep, Duration};

use telegram_raf::persistence::db::connection;
use telegram_raf::persistence::types::*;

use telegram_raf::telegram::commands::*;
use telegram_raf::telegram::handlers;

#[tokio::main]
async fn main() {
    SimpleLogger::new()
        .with_level(LevelFilter::Info)
        .init()
        .unwrap();

    let pool = connection();
    let token = env::var("TOKEN").expect("Provide the token via TOKEN env var");
    let bot_name = env::var("BOT_NAME").expect("Provide the bot name via BOT_NAME env var");

    // Check for the --broadcast flag
    let mut broadcast = false;
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 && args[1] == "--broadcast" {
        broadcast = true;
    }

    let mut binding = ClientBuilder::new();
    let mut client_builder = binding.set_token(&token);

    if broadcast {
        client_builder = client_builder.set_framework(create_framework!(&bot_name, broadcast))
    } else {
        client_builder = client_builder
            .set_framework(create_framework!(
                &bot_name, help, start, register, contest, list, rank
            ))
            .set_allowed_updates(vec![UpdateType::CallbackQuery, UpdateType::Message])
            .add_handler_func(handlers::message)
            .add_handler_func(handlers::callback);
    }

    let client = client_builder.build();

    {
        let mut data = client.data.write();
        data.insert::<DBKey>(pool);
        data.insert::<NameKey>(bot_name);
    }

    if broadcast {
        let ret = client.start().await;
        match ret {
            Err(err) => {
                error!("ApiResponse {}\nWaiting a minute and retrying...", err);
                sleep(Duration::from_secs(60)).await;
            }
            Ok(()) => {
                error!("Exiting from main loop without an error, but this should never happen!");
            }
        }
    } else {
        loop {
            let ret = client.start().await;
            match ret {
                Err(err) => {
                    error!("ApiResponse {}\nWaiting a minute and retrying...", err);
                    sleep(Duration::from_secs(60)).await;
                }
                Ok(()) => {
                    error!(
                        "Exiting from main loop without an error, but this should never happen!"
                    );
                    break;
                }
            }
        }
    }
}
