# Why I built AKM

AKM is a Windows program that keeps API keys in a KeePass file on my own computer.
This note is the account I would post. The repository is still private, so a post
should not link to it until I choose to publish the source. The facts below match
the README, the security policy and the review of 14 November 2026. Sentences about
why I wanted it are mine to edit.

## The short version

I was tired of API keys living in plain-text `.env` files. Those files get copied
into chats, into shell history, and into folders that an AI coding agent can read.
A password manager can hold a key, and then I still paste it into the project by
hand. Most of those managers are also an account on someone else's computer.

I wanted one encrypted file, in a format I did not invent, that another program can
still open if mine breaks. That format is KeePass. KeePassXC already opens it. What
it does not do is the part I do every day: this key belongs to this project and this
environment, it expires on this date, and this one program should receive it as an
environment variable. The rest of the computer should not.

AKM is that layer. A desktop app and a command-line tool, on a KeePass vault, with
no account and no server of mine.

## What it solves in this build

- The keys sit in one encrypted file. A stolen copy is not the keys, unless the
  thief also has the passphrase. There is no reset. A lost passphrase is a lost vault.
- A project and an environment select a set of keys. The command line, or **Open
  terminal with keys** in the app, starts one program with that set. The master
  password does not pass to the program. The rest of the shell does not receive the
  keys.
- Importing a `.env` file shows the names and waits for a confirmation. It does not
  show the values, and it writes nothing until I confirm.
- Copy asks Windows to leave the value out of clipboard history, cloud clipboard and
  monitoring. AKM tries to clear its own copy after 30 seconds, on lock and on a
  normal exit. A clipboard history program, or a busy clipboard, can keep the copy.
- The activity log stays on this computer, in a private folder. A cloud folder that
  syncs the vault gets the vault and its backups, not a log of which key I used.
- If I stop using AKM, KeePassXC 2.8 can still open the file. That interchange is
  tested. Older KeePass programs that cannot read KDBX 4.1 cannot.

It does not stop malware that runs as me, a keylogger, or a person at an unlocked
computer. It does not merge two copies of the vault. It does not phone home: AKM's
own code has no network client. The setup program can still download Microsoft's
WebView2 runtime if the computer does not have it. The KeePass library AKM uses
labels its KDBX 4.1 writer experimental. I reused that format on purpose. I did not
write a new cipher, and nobody has done a professional audit of this integration.

## What the score is, and what it is not

I had the code read by independent reviewers, against a written rubric, before I
treated it as finished. The latest of those reviews, on 14 November 2026, scored
**91.75 out of 100**. The same rubric scored 40.5 on 30 October, 66.25 on
9 November, and 83.5 then 87.5 on 13 November. The bar I set was above 90, and then
my own testing on a real Windows session.

91.75 is a reading of the source and the documents. The reviewers did not start the
app. Four of the six would use the command line, with precautions. Two would not yet
keep real keys in the desktop app. Every one of them wrote that the points overstate
the desktop app, because several of its flows have never run on a real Windows
session: the real clipboard, locking Windows, the import window, a real terminal
window, and an install on a clean computer. There is no publisher signature and no
public download.

So the number means the written review passed the bar I set. It does not mean the
app is certified, and it does not mean a stranger should put production keys in it.

## How to use it as it stands

The source folder is where the program is built and tested. The vault does not
belong there. Agents and Git commands work in that folder. A personal vault should
live somewhere else, and a second copy should live on another disk. AKM keeps dated
backups beside the vault, and a disk failure takes those with the vault.

1. Build or install from a build I made myself. The setup program is unsigned, so
   Windows shows **Windows protected your PC**. **More info**, then **Run anyway**,
   only for a file whose origin I trust. Setup installs for my Windows account and
   does not ask for an administrator.
2. Create the vault in a folder I chose, outside the source tree. The master
   password needs at least 12 characters. Longer and unique is the point. Write it
   down somewhere that is not the vault folder. Nobody can reset it.
3. Add one key I can throw away. Close AKM. Open the same file in KeePassXC 2.8 or
   later. If the key is there, the escape route works.
4. Add keys I can revoke and create again at the provider. Then a defect costs a
   rotation, not an outage.
5. Prefer **Open terminal with keys**, or `akm run` with a project and an
   environment, over leaving a `.env` file on disk. A saved `.env` file is plain
   text. Keep it out of Git and delete it when the job is done.
6. If I use **Copy**, turn off Windows clipboard history. AKM asks Windows to exclude
   the copy, and then tries to clear it. That is not a guarantee.
7. Copy the vault file to the other disk after each batch of new keys. Do not edit
   the same file on two computers at once. AKM will not merge a conflict.
8. When a provider rotates a key, change it at the provider first, then record the
   new value in AKM.

The command line is the path the automated tests have run on a real build. The
desktop app is the one I open. Its main screens work in the tests that do not need
me in the room. The flows in step 3 of the list above that need a real clipboard, a
real Windows lock or a real install have not had that run yet. The project's own
security policy still says: real secrets wait for that testing. I can choose to
start with keys I can replace. That choice is mine, and the policy does not make it
for a reader of this note.

## A post, if I want one paragraph

I built AKM because my API keys kept ending up in plain-text `.env` files, where
chats, shell history and coding agents can read them. It stores the keys in a
KeePass file on my own Windows PC, with no account, and it can hand one project's
keys to one program. KeePassXC can still open the file if I stop using AKM. An
independent review of the code scored it 91.75/100 on 14 November 2026. That is not
an audit. The tests that need a real desktop session have not run, the installer is
unsigned, and there is no public download yet. I am using it for keys I can revoke.

## What I would not say in a post

- I would not say it is audited, certified, or safe for a team's production secrets.
- I would not say the clipboard clear always works.
- I would not say the score proves the desktop app. The reviewers did not start it.
- I would not link the GitHub repository while it is private, and I would not publish
  the source until the release steps in `docs/RELEASING.md` are done. Those steps
  exist so a public copy does not carry a private path or a secret from this computer.
