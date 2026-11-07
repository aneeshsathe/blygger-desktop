# #thicket chat (experimental)

> **Experimental: this may be removed.** The chat tab is a trial. If not
> enough people use it, or people don't like it, a later release takes it
> out again.

Burrow can keep you in **#thicket**, a channel for Burrow users on
[Libera.Chat](https://libera.chat), an IRC network. It sits in its own tab
beside the views in the title bar, labelled **#thicket** with an
*experimental* badge.

- **⇧⌘I** (Blyg › #thicket Chat) brings the tab to the front; press it again
  to go back to your posts. On Windows the key is Ctrl+Shift+I.
- **Nothing connects to Libera.Chat until you set it up** in the tab.

## Setting up

The first time you open the tab it asks how you'd like to sign in:

1. **I have a Libera.Chat account.** Enter your nick (your account name) and
   password. Burrow signs in with SASL over TLS, as Libera.Chat recommends,
   and keeps the password only if Libera.Chat accepts it.
2. **Create an account.** Pick a nick, a password (at least 8 characters, no
   spaces) and a real email address. Your nick becomes your account name.
   Burrow connects as that nick and sends NickServ, Libera.Chat's account
   service, `REGISTER <password> <email>`, which is what Libera's guide has
   you type. NickServ's replies show under the form.
   Libera.Chat then emails you a line like
   `/msg NickServ VERIFY REGISTER ada_example abcdefgh`. Paste the code, or
   the whole line, into the **Code** field and press ⏎. Libera.Chat drops an
   account that isn't verified within about a day.

When it works, Burrow keeps the password in the macOS Keychain (Windows
Credential Manager on Windows), under the account `irc:libera:<nick>` beside
your blyg sign-ins, and writes `chat-nick = <nick>` to your config file. The
password is never written to a file. From then on Burrow signs in on its own
each time it opens.

**Sign out** in the tab's header forgets the password and the nick. Your
Libera.Chat account stays as it is.

## The tab

- **The log**: each line has its time and the speaker's nick. `/me` actions
  show as `* nick waves`. Joins, parts and nick changes are kept quiet, in
  small grey lines. A line that mentions your nick is marked with the accent
  colour.
- **The user list**, on the right: operators first, marked `@`, then voiced
  members (`+`), then everyone else.
- **The compose box**: ⏎ sends. `/me waves` sends an action; `//text` sends a
  line that starts with a slash. Other IRC commands aren't available here.
- **The connection** shows quietly in the header: connecting, signing in,
  connected, or "offline · retrying in 5 s". A dropped connection comes back
  on its own, waiting a little longer each time (2 s, 5 s, 15 s, 30 s, then
  every minute). A refused password doesn't retry: set up again.

## Marks and notifications

- While the tab isn't in front, a **dot and a count** on it say how many
  messages came in.
- When someone **mentions your nick** (or messages you privately), the tab
  shows a stronger accent **@** mark with the count, and Burrow posts a
  system notification (the Dock icon also bounces once if Burrow isn't the
  active app). macOS asks you to allow notifications the first time. On
  Windows only the tab's marks show for now.
- Opening the tab clears its marks.

## Settings

Settings (⌘,) › **Chat**:

- **Show #thicket tab** / **Hide it** (`chat-tab`, on by default). Hiding the
  tab also disconnects. It's on by default because it costs nothing until you
  set it up, and a trial nobody can find tells us nothing.
- **Notify me of mentions** (`chat-notifications`, on by default).

The config file also has `chat-nick` and `chat-server` (Libera.Chat over TLS,
`irc.libera.chat:6697`; only Libera.Chat is supported). See
[Configuration](config.md).

## Privacy

What you say in #thicket is public to everyone in the channel, and Libera.Chat
sees your connection as it would from any IRC client. The chat log is kept in
memory only (the last 1,000 lines) and is gone when Burrow quits. Burrow never
logs or prints your password.
