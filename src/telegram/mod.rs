//! `RaF` telegram communcation.
//!
//! This create is based upon a [fork (galeone/telexide) ](https://github.com/galeone/telexide/) of [telexide](https://github.com/CalliEve/telexide).
//! The fork made it possible to use the library, since it was not maintained anymore and it was
//! using a very old version of tokio.
//!
//! It uses the `raf::persistence` crate too, since every action executed remotely can have some
//! local effect on the `RaF` storage.
//!
//! # What's inside this crate?
//!
//! - `channels`: functions for working with channels, like registering the channels to `RaF` or
//! getting the channels info. Despite the name, also groups and supergroups are supported, even
//! though they are alwasy considered channels. Under the hood, there's almost zero differences
//! from the `RaF` goal.
//! - `commands`: the comands available to the `RaF` users, like `/start`, `/rank`, `/contest`. See
//! `/help` for the complete list of commands.
//! - `contests`: function for creating and updating the contests. The complete contest workflow is
//! not here, but in the `handlers` crate - because of how Telegram (and Telexide) works.
//! - `handlers`: the handlers for callback events (buttons, user interactions) and user messages.
//! - `messages`: functions for managing the text messages, like sending the `RaF` menu, working with
//! markdown, ...
//! - `users`: functions for getting a specific users or all the users that are channel owners.

pub mod channels;
pub mod commands;
pub mod contests;
pub mod handlers;
pub mod messages;
pub mod users;
