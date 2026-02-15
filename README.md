
# Setup

## Manual

You can run `smtpserver` and `pop3server` manualy if you like, but they will require root as they bind to low port numbers.

## Docker compose

You can use the docker compose file provided, or write your own if you like

# `smtpserver`

`smtpserver` is a server for receiving emails over SMTP. It is not a relay.

# `pop3server`

`pop3server` is a server for accessing mail stored by the `smtpserver` program. It provides basic features such as mail fetching and deletion.

## Options

 - `-h`, `--help` display help
 - `-f <path>`, `--db-path <path>` set the path of the database to use (defaults to /var/mail/mail.db)

# `pop3server`

 - `-h`, `--help` display help
 - `-f <path>`, `--db-path <path>` set the path of the database to use (defaults to /var/mail/mail.db)
