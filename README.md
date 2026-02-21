
# About

This product is probably not very good in comparison to a professional software package. If you don't want something robust, use a different program.
What it is, however is small, local and modular, meaning you just set up the services for what you need. It also uses a combined inbox system, where every account shares a global inbox, meaning you don't need an account to receive an email, allowing you to have as many email accounts as required.

# Setup

## Manual

You can run `smtpserver` and `pop3server` manually if you like, but they will require root as they bind to low port numbers.

## Docker compose

You can use the docker compose file provided, or write your own if you like. The provided `compose.yaml` is designed to go in the parent directory of this git repository, and creates services for the pop and smtp servers, as well as a relay.

## Accounts

If you want to use the pop3server, you will need to create an account. Install sqlite3 and use `sqlite3 db_path` to connect to the mail database. Once you are connected, you can run `INSERT INTO users (email_address,password) VALUES ("<your email here>","<your hashed password>");`. It uses sha256 as the hashing algorithm, so you could use `echo -n "mypass" | sha256sum` to generate the hashed password value. Once you have added an account, you can see the accounts using `SELECT * from users;`

## Manually viewing mail

Mail is stored in the `emails` table, so you can use `sqlite3` to query them manually or create a python script to retrieve emails for you.

# smtpserver

smtpserver is a server for receiving emails over SMTP. It is not a relay.

# pop3server

pop3server is a server for accessing mail stored by the smtpserver program. It provides basic features such as mail fetching and deletion.

# smtprelay

smtprelay is a relay server that accepts outbound mail on port 9185 and forwards it to the correct destination.

## Options

### smtpserver

 - `-h`, `--help` display help
 - `-f <path>`, `--db-path <path>` set the path of the database to use (defaults to /var/mail/mail.db)

### pop3server

 - `-h`, `--help` display help
 - `-f <path>`, `--db-path <path>` set the path of the database to use (defaults to /var/mail/mail.db)
