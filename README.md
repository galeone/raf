# Telegram RaF \[Refer a Friend\]([@RefafBot](https://t.me/RefafBot))

RaF is a bot for creating referral-based contests for your Telegram channels, groups and supergroups.

Create contests, let your users share their link to your channel/group, increase your audience, and give prizes to the winners!

---

## Introduction

The software is written in [rust](https://github.com/rust-lang/rust). Raf depends on [a fork of telexide](https://github.com/galeone/telexide), a rust library for making telegram bots. The fork makes the original library work and solves some issues.

The storage used is SQLite: RaF creates a `raf.db` file in its run path where it saves all the relationships between:

- Who owns the channels
- The contests created
- The invitations each participant generated
- The users who joined the channel through an invitation

## Setup

1. Install RaF

For the development version:

```bash
cargo install --path .
```

<!-- The stable version is not (yet) ready, especially because we depend on the fork that isn't published on crates.io but it's only a git repository. -->

For the production version:

```bash
cargo install telegram-raf
```

2. Create the run path and the environment file

```bash
mkdir $HOME/.raf

echo 'BOT_NAME="<your bot name>"' > $HOME/.raf/raf.env
echo 'TOKEN="<your bot token>"' >> $HOME/.raf/raf.env
```

3. Copy the systemd service file

```bash
sudo cp misc/systemd/raf@.service /lib/systemd/system/
```

4. Start and enable the service

```bash
sudo systemctl start raf@$USER.service
sudo systemctl enable raf@$USER.service
```

The `raf.db` (to backup or inspect) is in `$HOME/.raf/`.

### Broadcast Feature

The bot supports a broadcast feature that allows the bot owner to send messages to all users and channels. To use this feature:

1. Create a `broadcast.md` file in the bot's run directory (`$HOME/.raf/`) with the message you want to broadcast. The message supports Markdown formatting.

2. If the bot is currently running, stop it. It requires a separate instance. Now start the bot with the `--broadcast` flag:
```bash
raf --broadcast
```

3. Once the bot is running, use the `/broadcast` command to send the message from `broadcast.md` to all users and channels.
4. You can restart the bot to make it work as usual.

The `broadcast.md` file should be formatted using Markdown V2 syntax, as the bot will send the message with `ParseMode::MarkdownV2`.

## Contributing

Any feedback is welcome. Feel free to open issues and create pull requests!


## License

```
Copyright 2021 Paolo Galeone <nessuno@nerdz.eu>

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

   http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```
