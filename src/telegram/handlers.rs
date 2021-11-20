use chrono::{DateTime, Utc};
use data_encoding::BASE64URL;
use log::{error, info};
use rusqlite::params;
use tabular::{Row, Table};
use telexide::model::{
    Chat, ChatMember, InlineKeyboardButton, InlineKeyboardMarkup, ParseMode, ReplyMarkup,
    UpdateContent,
};
use telexide::{
    api::types::{AnswerCallbackQuery, GetChatMember, PinChatMessage, SendMessage},
    prelude::*,
};
use tokio::time::{sleep, Duration};

use crate::persistence::types::{Channel, Contest, DBKey, NameKey, User};
use crate::telegram::channels;
use crate::telegram::commands::start;
use crate::telegram::contests;
use crate::telegram::messages::{
    delete_parent_message, display_main_commands, display_manage_menu, escape_markdown,
    remove_loading_icon,
};
use crate::telegram::users;

#[prepare_listener]
pub async fn callback(ctx: Context, update: Update) {
    let callback = match update.content {
        UpdateContent::CallbackQuery(ref q) => q,
        _ => return,
    };
    let parent_message = callback.message.as_ref().map(|message| message.message_id);
    let chat_id = callback.message.clone().unwrap().chat.get_id();
    let sender_id = callback.from.id;

    let data = callback.data.clone().unwrap_or_else(|| "".to_string());
    let mut source: i64 = 0;
    let mut dest: i64 = 0;
    let chan_id: i64;
    // Accepted invitation
    let mut accepted = false;
    let mut manage = false;
    // Manage commands
    let (mut create, mut delete, mut stop, mut start, mut list) =
        (false, false, false, false, false);
    // Back to main menu
    let mut main = false;
    // Start/Stop/Delete Contest commands
    let (mut start_contest, mut delete_contest, mut stop_contest) = (false, false, false);
    let mut contest_id = 0;
    if data.contains('✅') {
        let mut iter = data.split_ascii_whitespace();
        iter.next(); // tick
        source = iter.next().unwrap().parse().unwrap(); // source user
        dest = iter.next().unwrap().parse().unwrap(); // dest user
        chan_id = iter.next().unwrap().parse().unwrap(); // channel id
        contest_id = iter.next().unwrap().parse().unwrap(); // contest id
        accepted = true;
    } else if data.contains('❌') {
        // Rejected invitation
        let text = Some("Ok, doing nothing.".to_string());
        let res = ctx
            .api
            .answer_callback_query(AnswerCallbackQuery {
                callback_query_id: callback.id.clone(),
                cache_time: None,
                show_alert: false,
                text,
                url: None,
            })
            .await;
        if res.is_err() {
            error!("[callback handler] {}", res.err().unwrap());
        }
        return;
    } else if data.starts_with("manage") {
        let mut iter = data.split_ascii_whitespace();
        iter.next(); // manage
        chan_id = iter.next().unwrap().parse().unwrap();
        manage = true;
    } else if data.starts_with("main") {
        let mut iter = data.split_ascii_whitespace();
        iter.next(); // main
        chan_id = iter.next().unwrap().parse().unwrap();
        main = true;
    } else if data.starts_with("create") {
        let mut iter = data.split_ascii_whitespace();
        iter.next(); // delete
        chan_id = iter.next().unwrap().parse().unwrap();
        create = true;
    } else if data.starts_with("delete_contest") {
        let mut iter = data.split_ascii_whitespace();
        iter.next(); // delete
        chan_id = iter.next().unwrap().parse().unwrap();
        contest_id = iter.next().unwrap().parse().unwrap();
        delete_contest = true;
    } else if data.starts_with("start_contest") {
        let mut iter = data.split_ascii_whitespace();
        iter.next(); // start
        chan_id = iter.next().unwrap().parse().unwrap();
        contest_id = iter.next().unwrap().parse().unwrap();
        start_contest = true;
    } else if data.starts_with("stop_contest") {
        let mut iter = data.split_ascii_whitespace();
        iter.next(); // start
        chan_id = iter.next().unwrap().parse().unwrap();
        contest_id = iter.next().unwrap().parse().unwrap();
        stop_contest = true;
    } else if data.starts_with("delete") {
        let mut iter = data.split_ascii_whitespace();
        iter.next(); // delete
        chan_id = iter.next().unwrap().parse().unwrap();
        delete = true;
    } else if data.starts_with("stop") {
        let mut iter = data.split_ascii_whitespace();
        iter.next(); // stop
        chan_id = iter.next().unwrap().parse().unwrap();
        stop = true;
    } else if data.starts_with("start") {
        let mut iter = data.split_ascii_whitespace();
        iter.next(); // start
        chan_id = iter.next().unwrap().parse().unwrap();
        start = true;
    } else if data.starts_with("list") {
        let mut iter = data.split_ascii_whitespace();
        iter.next(); // start
        chan_id = iter.next().unwrap().parse().unwrap();
        list = true;
    } else {
        // Anyway, on no-sense command reply with the empty message
        // to remove the loading icon next to the button
        let res = ctx
            .api
            .answer_callback_query(AnswerCallbackQuery {
                callback_query_id: callback.id.clone(),
                cache_time: None,
                show_alert: false,
                text: None,
                url: None,
            })
            .await;
        if res.is_err() {
            error!("[callback handler] {}", res.err().unwrap());
        }
        return;
    }

    if main {
        delete_parent_message(&ctx, chat_id, parent_message).await;
        display_main_commands(&ctx, sender_id).await;
        return;
    }

    let chan = {
        let guard = ctx.data.read();
        let map = guard.get::<DBKey>().expect("db");
        let conn = map.get().unwrap();

        let mut stmt = conn
            .prepare("SELECT link, name, registered_by FROM channels WHERE id = ?")
            .unwrap();

        let channel = stmt
            .query_map(params![chan_id], |row| {
                Ok(Channel {
                    id: chan_id,
                    link: row.get(0)?,
                    name: row.get(1)?,
                    registered_by: row.get(2)?,
                })
            })
            .unwrap()
            .map(std::result::Result::unwrap)
            .next()
            .unwrap();
        Some(channel)
    };

    if chan.is_none() {
        return;
    }
    let chan = chan.unwrap();

    if accepted {
        // getChatMember always returns a ChatMember, even if the user never joined the chan.
        // if the request fails, the user does not exists and we should exit
        // if the request is ok, we need to check the type of the ChatMember
        let member = ctx
            .api
            .get_chat_member(GetChatMember {
                chat_id: chan.id,
                user_id: sender_id,
            })
            .await;

        let member_joined = |m: ChatMember| -> bool {
            match m {
                ChatMember::Administrator(_)
                | ChatMember::Creator(_)
                | ChatMember::Member(_)
                | ChatMember::Restricted(_) => true,
                ChatMember::Kicked(_) | ChatMember::Left(_) => false,
            }
        };
        match member {
            Ok(m) => {
                if member_joined(m) {
                    let text = format!(
                        "You are already a member of [{}]({})\\.",
                        escape_markdown(&chan.name.to_string(), None),
                        chan.link
                    );
                    let mut reply = SendMessage::new(sender_id, &text);
                    reply.set_parse_mode(&ParseMode::MarkdownV2);
                    let res = ctx.api.send_message(reply).await;
                    if res.is_err() {
                        let err = res.err().unwrap();
                        error!("[already member] {}", err);
                    }
                    remove_loading_icon(&ctx, &callback.id, None).await;
                    return;
                }
            }
            Err(err) => {
                let text = escape_markdown(&format!("{}", err), None);
                let mut reply = SendMessage::new(sender_id, &text);
                reply.set_parse_mode(&ParseMode::MarkdownV2);
                let res = ctx.api.send_message(reply).await;
                if res.is_err() {
                    let err = res.err().unwrap();
                    error!("[already member] {}", err);
                }
                remove_loading_icon(&ctx, &callback.id, None).await;
                return;
            }
        }

        let res = ctx
            .api
            .answer_callback_query(AnswerCallbackQuery {
                callback_query_id: callback.id.clone(),
                cache_time: None,
                show_alert: false,
                text: None,
                url: None,
            })
            .await;
        if res.is_err() {
            error!("[callback handler] {}", res.err().unwrap());
        }
        let text = format!(
            "Please join \u{1f449} [{}]({}) within the next 10 seconds\\.",
            escape_markdown(&chan.name.to_string(), None),
            chan.link
        );
        let mut reply = SendMessage::new(sender_id, &text);
        reply.set_parse_mode(&ParseMode::MarkdownV2);
        let res = ctx.api.send_message(reply).await;
        if res.is_err() {
            let err = res.err().unwrap();
            error!("[please join] {}", err);
        }

        sleep(Duration::from_secs(10)).await;
        let member = ctx
            .api
            .get_chat_member(GetChatMember {
                chat_id: chan.id,
                user_id: sender_id,
            })
            .await;

        // The unwrap is likely to not fail, since the previous request is identical and succeded
        let joined = member_joined(member.unwrap());
        if joined {
            info!("Refer OK!");
            let c = contests::get(&ctx, contest_id);
            if c.is_none() {
                error!("[refer ok] Invalid contest passed in url");
                let res = ctx
                    .api
                    .send_message(SendMessage::new(
                        sender_id,
                        "You joined the channel but the contest does not exist.",
                    ))
                    .await;
                if res.is_err() {
                    let err = res.err().unwrap();
                    error!("[failed to insert invitation] {}", err);
                }
            } else {
                let c = c.unwrap();
                let now: DateTime<Utc> = Utc::now();
                if now > c.end {
                    info!("Joining with expired contest");
                    let res = ctx
                        .api
                        .send_message(SendMessage::new(
                            sender_id,
                            "You joined the group/channel but the contest is finished",
                        ))
                        .await;
                    if res.is_err() {
                        let err = res.err().unwrap();
                        error!("[failed to insert invitation] {}", err);
                    }
                } else {
                    let res = {
                        let guard = ctx.data.read();
                        let map = guard.get::<DBKey>().expect("db");
                        let conn = map.get().unwrap();
                        conn.execute(
                            "INSERT INTO invitations(source, dest, chan, contest) VALUES(?, ?, ?, ?)",
                            params![source, dest, chan.id, contest_id],
                        )
                    };
                    if res.is_err() {
                        let err = res.err().unwrap();
                        error!("[insert invitation] {}", err);
                        let res = ctx
                            .api
                            .send_message(SendMessage::new(
                                sender_id,
                                "Failed to insert invitation: this invitation might already exist!",
                            ))
                            .await;
                        if res.is_err() {
                            let err = res.err().unwrap();
                            error!("[failed to insert invitation] {}", err);
                        }
                    } else {
                        let text = format!(
                            "You joined [{}]({}) \u{1f917}",
                            escape_markdown(&chan.name.to_string(), None),
                            chan.link
                        );
                        let mut reply = SendMessage::new(sender_id, &text);
                        reply.set_parse_mode(&ParseMode::MarkdownV2);
                        let res = ctx.api.send_message(reply).await;
                        if res.is_err() {
                            let err = res.err().unwrap();
                            error!("[joined send] {}", err);
                        }
                    }
                }
            }
        } else {
            info!("User not joined the channel after 10 seconds...");
            let text = escape_markdown("You haven't joined the channel within 10 seconds :(", None);
            let mut reply = SendMessage::new(sender_id, &text);
            reply.set_parse_mode(&ParseMode::MarkdownV2);
            let res = ctx.api.send_message(reply).await;
            if res.is_err() {
                let err = res.err().unwrap();
                error!("[not join] {}", err);
            }
        }
        delete_parent_message(&ctx, chat_id, parent_message).await;
    }

    if manage {
        remove_loading_icon(&ctx, &callback.id, None).await;
        display_manage_menu(&ctx, chat_id, &chan).await;
        delete_parent_message(&ctx, chat_id, parent_message).await;
    }

    if start {
        let contests = contests::get_all(&ctx, chan.id)
            .into_iter()
            .filter(|c| c.started_at.is_none())
            .collect::<Vec<Contest>>();
        if contests.is_empty() {
            remove_loading_icon(&ctx, &callback.id, Some("You have no contests to start!")).await;
        } else {
            let mut reply = SendMessage::new(
                sender_id,
                &escape_markdown("Select the contest to start", None),
            );
            let mut partition_size: usize = contests.len() / 2;
            if partition_size < 2 {
                partition_size = 1;
            }
            let inline_keyboard: Vec<Vec<InlineKeyboardButton>> = contests
                .chunks(partition_size)
                .map(|chunk| {
                    chunk
                        .iter()
                        .map(|contest| InlineKeyboardButton {
                            text: contest.name.clone(),
                            // delete_contest, channel id, contest id
                            callback_data: Some(format!(
                                "start_contest {} {}",
                                chan.id, contest.id
                            )),
                            callback_game: None,
                            login_url: None,
                            pay: None,
                            switch_inline_query: None,
                            switch_inline_query_current_chat: None,
                            url: None,
                        })
                        .collect()
                })
                .collect();
            reply.set_reply_markup(&ReplyMarkup::InlineKeyboardMarkup(InlineKeyboardMarkup {
                inline_keyboard,
            }));
            reply.set_parse_mode(&ParseMode::MarkdownV2);

            let res = ctx.api.send_message(reply).await;
            if res.is_err() {
                let err = res.err().unwrap();
                error!("[start send] {}", err);
            }
            remove_loading_icon(&ctx, &callback.id, None).await;
            delete_parent_message(&ctx, chat_id, parent_message).await;
        };
    }

    if stop {
        let contests = contests::get_all(&ctx, chan.id)
            .into_iter()
            .filter(|c| c.started_at.is_some() && !c.stopped)
            .collect::<Vec<Contest>>();
        if contests.is_empty() {
            remove_loading_icon(&ctx, &callback.id, Some("You have no contests to stop!")).await;
        } else {
            let mut reply = SendMessage::new(
                chat_id,
                &escape_markdown("Select the contest to stop", None),
            );
            let mut partition_size: usize = contests.len() / 2;
            if partition_size < 2 {
                partition_size = 1;
            }
            let inline_keyboard: Vec<Vec<InlineKeyboardButton>> = contests
                .chunks(partition_size)
                .map(|chunk| {
                    chunk
                        .iter()
                        .map(|contest| InlineKeyboardButton {
                            text: contest.name.clone(),
                            // stop_contest, channel id, contest id
                            callback_data: Some(format!("stop_contest {} {}", chan.id, contest.id)),
                            callback_game: None,
                            login_url: None,
                            pay: None,
                            switch_inline_query: None,
                            switch_inline_query_current_chat: None,
                            url: None,
                        })
                        .collect()
                })
                .collect();
            reply.set_reply_markup(&ReplyMarkup::InlineKeyboardMarkup(InlineKeyboardMarkup {
                inline_keyboard,
            }));
            reply.set_parse_mode(&ParseMode::MarkdownV2);

            let res = ctx.api.send_message(reply).await;
            if res.is_err() {
                let err = res.err().unwrap();
                error!("[create send]  {}", err);
            }
            remove_loading_icon(&ctx, &callback.id, None).await;
            delete_parent_message(&ctx, chat_id, parent_message).await;
        };
    }

    if stop_contest {
        // Clean up ranks from users that joined and then left the channel
        let c = contests::get(&ctx, contest_id).unwrap();
        if c.stopped {
            let reply = SendMessage::new(chat_id, "Contest already stopped. Doing nothing.");
            let res = ctx.api.send_message(reply).await;
            if res.is_err() {
                let err = res.err().unwrap();
                error!("[stop send] {}", err);
            }
            display_manage_menu(&ctx, chat_id, &chan).await;
            delete_parent_message(&ctx, chat_id, parent_message).await;
        } else {
            contests::validate_users(&ctx, &c).await;

            // Stop contest on db
            let c = {
                let guard = ctx.data.read();
                let map = guard.get::<DBKey>().expect("db");
                let conn = map.get().unwrap();
                let mut stmt = conn.prepare("UPDATE contests SET stopped = TRUE WHERE id = ? RETURNING name, prize, end, started_at").unwrap();
                let mut iter = stmt
                    .query_map(params![contest_id], |row| {
                        Ok(Contest {
                            id: contest_id,
                            name: row.get(0)?,
                            prize: row.get(1)?,
                            end: row.get(2)?,
                            started_at: row.get(3)?,
                            stopped: true,
                            chan: chan.id,
                        })
                    })
                    .unwrap();
                iter.next().unwrap().unwrap()
            };

            // Create rank
            let rank = contests::ranking(&ctx, &c);
            if rank.is_empty() {
                // No one partecipated in the challenge
                let reply = SendMessage::new(
                    sender_id,
                    "No one partecipated to the challenge. Doing nothing.",
                );
                let res = ctx.api.send_message(reply).await;
                if res.is_err() {
                    let err = res.err().unwrap();
                    error!("[stop send] {}", err);
                }
                display_manage_menu(&ctx, chat_id, &chan).await;
                delete_parent_message(&ctx, chat_id, parent_message).await;
            } else {
                // Send top-10 to the channel and pin the message
                let mut m = format!("\u{1f3c6} Contest ({}) finished \u{1f3c6}\n\n\n", c.name);
                let winner = rank[0].user.clone();
                for row in rank {
                    let user = row.user;
                    let rank = row.rank;
                    let invites = row.invites;
                    if rank == 1 {
                        m += "\u{1f947}#1!";
                    } else if rank <= 3 {
                        m += &format!("\u{1f3c6} #{}", rank);
                    } else {
                        m += &format!("#{}", rank);
                    }

                    m += &format!(
                        " {}{}{} - {}\n",
                        user.first_name,
                        match user.last_name {
                            Some(last_name) => format!(" {}", last_name),
                            None => "".to_string(),
                        },
                        match user.username {
                            Some(username) => format!(" ({})", username),
                            None => "".to_string(),
                        },
                        invites
                    );
                }
                m += &format!(
                    "\n\nThe prize ({}) is being delivered to our champion \u{1f947}. Congratulations!!",
                    c.prize
                );

                m = escape_markdown(&m, None);

                let mut reply = SendMessage::new(c.chan, &m);
                reply.set_parse_mode(&ParseMode::MarkdownV2);
                let res = ctx.api.send_message(reply).await;
                if res.is_err() {
                    let err = res.unwrap_err();
                    error!("[send message] {}", err);
                } else {
                    // Pin message
                    let res = ctx
                        .api
                        .pin_chat_message(PinChatMessage {
                            chat_id: c.chan,
                            message_id: res.unwrap().message_id,
                            disable_notification: false,
                        })
                        .await;
                    if res.is_err() {
                        let err = res.unwrap_err();
                        error!("[stop pin message] {}", err);
                        let reply = SendMessage::new(sender_id, &err.to_string());
                        let res = ctx.api.send_message(reply).await;
                        if res.is_err() {
                            error!("[stop pin message2] {}", res.unwrap_err());
                        }
                    }
                }

                // Put into communication the bot user and the winner
                let direct_communication = winner.username.is_some();
                let text = if direct_communication {
                    let username = winner.username.unwrap();
                    format!(
                        "The winner usename is @{}. Get in touch and send the prize!",
                        username
                    )
                } else {
                    "The winner has no username. It means you can communicate only through the bot.\n\n\
                Write NOW a message that will be delivered to the winner (if you can, just send the prize!).\n\n
                NOTE: You can only send up to one message, hence a good idea is to share your username with the winner\
                in order to make they start a commucation with you in private.".to_string()
                };
                let mut reply = SendMessage::new(sender_id, &escape_markdown(&text, None));
                reply.set_parse_mode(&ParseMode::MarkdownV2);
                let res = ctx.api.send_message(reply).await;
                if res.is_err() {
                    let err = res.err().unwrap();
                    error!("[stop send] {}", err);
                }
                if !direct_communication {
                    // Outside of FSM
                    let res = {
                        let guard = ctx.data.read();
                        let map = guard.get::<DBKey>().expect("db");
                        let conn = map.get().unwrap();
                        // add user to contact, the owner (me), the contest
                        // in order to add more constraint to verify outside of this FMS
                        // to validate and put the correct owner in contact with the correct winner
                        conn.execute(
                            "INSERT INTO being_contacted_users(user, owner) VALUES(?, ?)",
                            params![winner.id, sender_id],
                        )
                    };

                    if res.is_err() {
                        let err = res.err().unwrap();
                        error!("[insert being_contacted_users] {}", err);
                    }
                }
            }
        }

        remove_loading_icon(&ctx, &callback.id, None).await;
    }

    if create {
        let now: DateTime<Utc> = Utc::now();
        let mut reply = SendMessage::new(
            sender_id,
            &escape_markdown(
                &format!(
                    "Write a single message with every required info on a new line\n\n\
                Contest name\n\
                End date (YYY-MM-DD hh:mm TZ)\n\
                Prize\n\n\
                For example a valid message is (note the GMT+1 timezone written as +01):\n\n\
                {month_string} {year}\n\
                {year}-{month}-28 20:00 +01\n\
                Amazon 50\u{20ac} Gift Card\n",
                    year = now.format("%Y"),
                    month = now.format("%m"),
                    month_string = now.format("%B")
                ),
                None,
            ),
        );
        reply.set_parse_mode(&ParseMode::MarkdownV2);

        let res = ctx.api.send_message(reply).await;
        if res.is_err() {
            let err = res.err().unwrap();
            error!("[create send] {}", err);
        }

        // adding chan to being_managed_channels since the raw
        // reply falls outiside this FSM
        let res = {
            let guard = ctx.data.read();
            let map = guard.get::<DBKey>().expect("db");
            let conn = map.get().unwrap();
            conn.execute(
                "INSERT INTO being_managed_channels(chan) VALUES(?)",
                params![chan.id],
            )
        };

        if res.is_err() {
            let err = res.err().unwrap();
            error!("[insert being_managed_channels] {}", err);
        }

        remove_loading_icon(&ctx, &callback.id, None).await;
        delete_parent_message(&ctx, chat_id, parent_message).await;
    }

    if delete {
        let contests = contests::get_all(&ctx, chan.id);
        if contests.is_empty() {
            remove_loading_icon(&ctx, &callback.id, Some("You have no contests to delete!")).await;
        } else {
            let mut reply = SendMessage::new(
                sender_id,
                &escape_markdown("Select the contest to delete", None),
            );
            let mut partition_size: usize = contests.len() / 2;
            if partition_size < 2 {
                partition_size = 1;
            }
            let inline_keyboard: Vec<Vec<InlineKeyboardButton>> = contests
                .chunks(partition_size)
                .map(|chunk| {
                    chunk
                        .iter()
                        .map(|contest| InlineKeyboardButton {
                            text: contest.name.clone(),
                            // delete_contest, channel id, contest id
                            callback_data: Some(format!(
                                "delete_contest {} {}",
                                chan.id, contest.id
                            )),
                            callback_game: None,
                            login_url: None,
                            pay: None,
                            switch_inline_query: None,
                            switch_inline_query_current_chat: None,
                            url: None,
                        })
                        .collect()
                })
                .collect();
            reply.set_reply_markup(&ReplyMarkup::InlineKeyboardMarkup(InlineKeyboardMarkup {
                inline_keyboard,
            }));
            reply.set_parse_mode(&ParseMode::MarkdownV2);

            let res = ctx.api.send_message(reply).await;
            if res.is_err() {
                let err = res.err().unwrap();
                error!("[create send] {}", err);
            }
            remove_loading_icon(&ctx, &callback.id, None).await;
            delete_parent_message(&ctx, chat_id, parent_message).await;
        };
    }

    if list {
        let text = {
            let contests = contests::get_all(&ctx, chan.id);
            let mut text: String = "".to_string();
            if !contests.is_empty() {
                text += "```\n";
                let mut table = Table::new("{:<} | {:<} | {:<} | {:<} | {:<} | {:<}");
                table.add_row(
                    Row::new()
                        .with_cell("Name")
                        .with_cell("End")
                        .with_cell("Prize")
                        .with_cell("Started")
                        .with_cell("Stopped")
                        .with_cell("Users"),
                );
                for (_, contest) in contests.iter().enumerate() {
                    let users = contests::count_users(&ctx, contest);
                    table.add_row(
                        Row::new()
                            .with_cell(&contest.name)
                            .with_cell(contest.end)
                            .with_cell(&contest.prize)
                            .with_cell(match contest.started_at {
                                Some(x) => format!("{}", x),
                                None => "No".to_string(),
                            })
                            .with_cell(if contest.stopped {
                                "Yes".to_string()
                            } else {
                                "No".to_string()
                            })
                            .with_cell(users),
                    );
                }
                text += &format!(
                    "{}```\n\n{}",
                    table,
                    escape_markdown(
                        "Dates are all converted to UTC timezone.\nBetter view on desktop.",
                        None
                    )
                );
            }
            text
        };

        if text.is_empty() {
            remove_loading_icon(
                &ctx,
                &callback.id,
                Some("You don't have any active or past contests for this group/channel!"),
            )
            .await;
        } else {
            let mut reply = SendMessage::new(sender_id, &text);
            reply.set_parse_mode(&ParseMode::MarkdownV2);

            let res = ctx.api.send_message(reply).await;

            if res.is_err() {
                let err = res.err().unwrap();
                error!("[list contests] {}", err);
            }
            remove_loading_icon(&ctx, &callback.id, None).await;

            display_manage_menu(&ctx, chat_id, &chan).await;
            delete_parent_message(&ctx, chat_id, parent_message).await;
        }
    }

    if delete_contest {
        let res = {
            let guard = ctx.data.read();
            let map = guard.get::<DBKey>().expect("db");
            let conn = map.get().unwrap();
            let mut stmt = conn.prepare("DELETE FROM contests WHERE id = ?").unwrap();
            stmt.execute(params![contest_id])
        };
        let text = if res.is_err() {
            let err = res.unwrap_err();
            error!("[delete from contests] {}", err);
            format!("Error: {}. You can't stop a contest with already some partecipant, this is unfair!", err)
        } else {
            "Done!".to_string()
        };
        let res = ctx
            .api
            .send_message(SendMessage::new(sender_id, &text))
            .await;
        if res.is_err() {
            let err = res.err().unwrap();
            error!("[send message delete contest] {}", err);
        }

        remove_loading_icon(&ctx, &callback.id, None).await;
        display_manage_menu(&ctx, chat_id, &chan).await;
        delete_parent_message(&ctx, chat_id, parent_message).await;
    }

    if start_contest {
        // if contest_id is not valid, this panics (that's ok, the user is doing nasty things)
        let c = contests::get(&ctx, contest_id).unwrap();
        if c.started_at.is_some() {
            let text = "You can't start an already started contest.";
            let res = ctx
                .api
                .send_message(SendMessage::new(sender_id, text))
                .await;
            if res.is_err() {
                let err = res.err().unwrap();
                error!("[send message] {}", err);
            }
        } else {
            let c = {
                let now: DateTime<Utc> = Utc::now();
                let guard = ctx.data.read();
                let map = guard.get::<DBKey>().expect("db");
                let conn = map.get().unwrap();
                let mut stmt = conn.prepare("UPDATE contests SET started_at = ? WHERE id = ? RETURNING name, prize, end").unwrap();
                let mut iter = stmt
                    .query_map(params![now, contest_id], |row| {
                        Ok(Contest {
                            id: contest_id,
                            name: row.get(0)?,
                            prize: row.get(1)?,
                            end: row.get(2)?,
                            started_at: Some(now),
                            stopped: false,
                            chan: chan.id,
                        })
                    })
                    .unwrap();
                iter.next().unwrap()
            };
            let text = if c.is_err() {
                let err = c.as_ref().err().unwrap();
                error!("[update/start contest] {}", err);
                err.to_string()
            } else {
                "Contest started!".to_string()
            };
            let res = ctx
                .api
                .send_message(SendMessage::new(sender_id, &text))
                .await;
            if res.is_err() {
                let err = res.err().unwrap();
                error!("[send message] {}", err);
            }

            if !c.is_err() {
                let c = c.unwrap();
                // Send message in the channel, indicating the contest name
                // the end date, the prize, and pin it on top until the end date comes
                // or the contest is stopped or deleted
                let bot_name = {
                    let guard = ctx.data.read();
                    guard
                        .get::<NameKey>()
                        .expect("name")
                        .clone()
                        .replace('@', "")
                };
                let params =
                    BASE64URL.encode(format!("chan={}&contest={}", chan.id, c.id).as_bytes());
                let text = format!(
                    "{title}\n\n{rules}\n\n{bot_link}",
                    title = escape_markdown(
                        &format!(
                            "\u{1f525}{name} contest \u{1f525}\nWho invites more friends wins a {prize}!",
                            prize = c.prize,
                            name = c.name
                        ),
                        None
                    ),
                    rules = format!(
                        "{} **{prize}**\n{disclaimer}",
                        escape_markdown(
                            &format!(
                                "1. Start the contest bot using the link below\n\
                            2. The bot gives you a link\n\
                            3. Share the link with your friends!\n\n\
                            At the end of the contest ({end_date}) the user that referred more friends \
                            will win a ",
                                end_date = c.end
                            ),
                            None
                        ),
                        prize = escape_markdown(&c.prize, None),
                        disclaimer =
                            escape_markdown("You can check your rank with the /rank command", None),
                    ),
                    bot_link = escape_markdown(
                        &format!(
                            "https://t.me/{bot_name}?start={params}",
                            bot_name = bot_name,
                            params = params
                        ),
                        None
                    ),
                );

                let mut reply = SendMessage::new(c.chan, &text);
                reply.set_parse_mode(&ParseMode::MarkdownV2);
                let res = ctx.api.send_message(reply).await;
                if res.is_err() {
                    let err = res.unwrap_err();
                    error!("[send message] {}", err);
                } else {
                    // Pin message
                    let res = ctx
                        .api
                        .pin_chat_message(PinChatMessage {
                            chat_id: c.chan,
                            message_id: res.unwrap().message_id,
                            disable_notification: false,
                        })
                        .await;
                    if res.is_err() {
                        let err = res.unwrap_err();
                        error!("[pin message] {}", err);
                        let reply = SendMessage::new(sender_id, &err.to_string());
                        let res = ctx.api.send_message(reply).await;
                        if res.is_err() {
                            error!("[pin message2] {}", res.unwrap_err());
                        }
                    }
                }
            }
        }

        remove_loading_icon(&ctx, &callback.id, None).await;
        display_manage_menu(&ctx, chat_id, &chan).await;
        delete_parent_message(&ctx, chat_id, parent_message).await;
    }
}

#[prepare_listener]
pub async fn message(ctx: Context, update: Update) {
    info!("message handler begin");
    let message = match update.content {
        UpdateContent::Message(ref m) => m,
        _ => return,
    };
    let sender_id = message.from.clone().unwrap().id;

    // If the user if forwarding a message from a channel, we are in the registration flow.
    // NOTE: we can extract info from the source chat, only in case of channels.
    // For (super)groups we need to have the bot inside the (super)group and receive
    // a message.
    let chat_id: Option<i64> = {
        if let Some(forward_data) = &message.forward_data {
            if let Some(from_chat) = &forward_data.from_chat {
                match from_chat {
                    Chat::Channel(c) => Some(c.id),
                    _ => None,
                }
            } else {
                None
            }
        } else {
            None
        }
    };

    let registration_flow = chat_id.is_some();
    if registration_flow {
        let chat_id = chat_id.unwrap();
        let registered_by = message.from.clone().unwrap().id;
        channels::try_register(&ctx, chat_id, registered_by).await;
        display_main_commands(&ctx, sender_id).await;
    } else {
        // If we are not in the channel registration flow, we just received a message
        // and we should check if the message is among the accepted ones.
        //
        // It can be a group registration flow, or a channel begin managed, or other.
        let text = message.get_text();
        if text.is_none() {
            return;
        }
        let text = text.unwrap();
        // We also receive commands in this handler, we need to skip them or correctly forward them
        // in case of commands that are used inside groups, e.g.
        // /start@bot_name
        if text.starts_with('/') {
            let owners = users::owners(&ctx)
                .iter()
                .map(|u| u.id)
                .collect::<Vec<i64>>();
            let is_owner = owners.iter().any(|&id| id == sender_id);
            let bot_name = {
                let guard = ctx.data.read();
                guard
                    .get::<NameKey>()
                    .expect("name")
                    .clone()
                    .replace('@', "")
            };
            if text.starts_with(&format!("/start@{}", bot_name)) && is_owner {
                let res = start(ctx, message.clone()).await;
                if res.is_err() {
                    error!("[inner start] {:?}", res.unwrap_err());
                }
            } else {
                let commands = vec!["help", "register", "contest", "list", "rank"];
                for command in commands {
                    if text.starts_with(&format!("/{}@{}", command, bot_name)) {
                        let chat_id = message.chat.get_id();
                        let text =  format!("All the commands, except for /start are disabled in groups. /start is enabled only for the group owner.\n\nTo use them, start @{}", bot_name);
                        let res = ctx.api.send_message(SendMessage::new(chat_id, &text)).await;

                        if res.is_err() {
                            let err = res.err().unwrap();
                            error!("[disabled commands in groups] {}", err);
                        }
                        break;
                    }
                }
            }
            return;
        }

        // From here below, we are interested only in messages sent from owners
        let owners = users::owners(&ctx)
            .iter()
            .map(|u| u.id)
            .collect::<Vec<i64>>();
        let is_owner = owners.iter().any(|&id| id == sender_id);
        if !is_owner {
            return;
        }

        // Check if some of the user channel's are being managed
        // in that case it's plausible that the user is sending the message in this format
        // ```
        // contest name
        // end date (YYYY-MM-DD hh:mm TZ)
        // prize
        // ```
        if text.split('\n').skip_while(|r| r.is_empty()).count() == 3 {
            let channels = channels::get(&ctx, sender_id); // channels registered by the user
            let chan = {
                let guard = ctx.data.read();
                let map = guard.get::<DBKey>().expect("db");
                let conn = map.get().unwrap();
                // In the begin_managed_channels we have all the channels ever managed, we can order
                // them by ID and keep only tha latest one, since there can be only one managed channel
                // at a time, by the same user.
                let mut stmt = conn
                    .prepare(&format!(
                        "SELECT channels.id, channels.link, channels.name, channels.registered_by FROM \
                        channels INNER JOIN being_managed_channels ON channels.id = being_managed_channels.chan \
                        WHERE being_managed_channels.chan IN ({}) ORDER BY being_managed_channels.id DESC LIMIT 1",
                        channels
                            .iter()
                            .map(|c| c.id.to_string())
                            .collect::<Vec<String>>()
                            .join(",")
                    ))
                    .unwrap();
                let chan = stmt
                    .query_map(params![], |row| {
                        Ok(Channel {
                            id: row.get(0)?,
                            link: row.get(1)?,
                            name: row.get(2)?,
                            registered_by: row.get(3)?,
                        })
                    })
                    .unwrap()
                    .map(Result::unwrap)
                    .next();
                chan
            };
            if chan.is_some() {
                let chan = chan.unwrap();
                let contest = contests::from_text(&text, chan.id);

                if let Ok(contest) = contest {
                    let res = {
                        let guard = ctx.data.read();
                        let map = guard.get::<DBKey>().expect("db");
                        let conn = map.get().unwrap();
                        conn.execute(
                            "INSERT INTO contests(name, end, prize, chan) VALUES(?, ?, ?, ?)",
                            params![contest.name, contest.end, contest.prize, contest.chan],
                        )
                    };

                    let text = if res.is_err() {
                        let err = res.err().unwrap();
                        error!("[insert contest] {}", err);
                        format!("Error: {}", err)
                    } else {
                        format!("Contest {} created succesfully!", contest.name)
                    };
                    let res = ctx
                        .api
                        .send_message(SendMessage::new(sender_id, &text))
                        .await;

                    if res.is_err() {
                        let err = res.err().unwrap();
                        error!("[contest ok send] {}", err);
                    }
                } else {
                    let err = contest.unwrap_err();
                    let res = ctx
                        .api
                        .send_message(SendMessage::new(
                            sender_id,
                            &format!(
                                "Something wrong happened while creating your new contest.\n\n\
                            Error: {}\n\n\
                            Please restart the contest creating process and send a correct message",
                                err
                            ),
                        ))
                        .await;

                    if res.is_err() {
                        let err = res.err().unwrap();
                        error!("[contest ok send] {}", err);
                    }
                }
                // No need to delete the currently beign managed channel. We alwasy look for the last
                // "being managed" inserted by this user
                // NOTE: use sender_id instead of chat_id because this must go on the private chat
                // user<->bot and not in the public chat.
                display_manage_menu(&ctx, sender_id, &chan).await;

                // else, if no channel is being edited, but we received a 3 lines message
                // it's just a message, do nothing (?)
            }
        } else {
            // text splitted in a number of rows != 3 -> it can be a message
            // being sent from an owner to a winner
            let winner = {
                let guard = ctx.data.read();
                let map = guard.get::<DBKey>().expect("db");
                let conn = map.get().unwrap();
                // In the being_contacted_users we have all the winner to be ever contacted
                // we can join the contest and the owner and filter with the current user_id
                // limiting only by the last one that matches all these conditions, to be almost
                // sure to link the owner with the winner (correct pair)
                let mut stmt = conn
                        .prepare(
                            "SELECT users.id, users.first_name, users.last_name, users.username FROM users \
                            INNER JOIN being_contacted_users ON users.id = being_contacted_users.user \
                            WHERE being_contacted_users.owner = ? AND being_contacted_users.contacted IS FALSE \
                            ORDER BY being_contacted_users.id DESC LIMIT 1"
                        )
                        .unwrap();
                let user = stmt
                    .query_map(params![sender_id], |row| {
                        Ok(User {
                            id: row.get(0)?,
                            first_name: row.get(1)?,
                            last_name: row.get(2)?,
                            username: row.get(3)?,
                        })
                    })
                    .unwrap()
                    .map(Result::unwrap)
                    .next();
                user
            };
            if winner.is_some() {
                let winner = winner.unwrap();
                let mut reply = SendMessage::new(winner.id, &text);
                reply.set_parse_mode(&ParseMode::MarkdownV2);
                let res = ctx.api.send_message(reply).await;
                if res.is_err() {
                    let err = res.err().unwrap();
                    error!("[winner communication] {}", err);
                } else {
                    let reply = SendMessage::new(sender_id, "Message delivered to the winner!");
                    let res = ctx.api.send_message(reply).await;
                    if res.is_err() {
                        let err = res.err().unwrap();
                        error!("[winner postcom] {}", err);
                    }
                    // Set the winner user as contacted
                    let res = {
                        let guard = ctx.data.read();
                        let map = guard.get::<DBKey>().expect("db");
                        let conn = map.get().unwrap();
                        // add user to contact, the owner (me), the contest
                        // in order to add more constraint to verify outside of this FMS
                        // to validate and put the correct owner in contact with the correct winner
                        conn.execute(
                            "UPDATE being_contacted_users SET contacted = TRUE WHERE owner = ? AND user = ?",
                            params![sender_id, winner.id],
                        )
                    };

                    if res.is_err() {
                        let err = res.err().unwrap();
                        error!("[insert being_contacted_users] {}", err);
                    }
                }

                display_main_commands(&ctx, sender_id).await;
            }
        }
    }

    info!("message handler end");
}
