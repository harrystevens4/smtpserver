
# About

This product is probably not very good in comparison to a professional software package. If you want something robust, use a different program.
What it is, however is small, local and modular, meaning you just set up the services for what you need. It also uses a combined inbox system, where every account shares a global inbox, meaning you don't need an account to receive an email, allowing you to have as many email accounts as required.

# Setup

## Manual

You can run `smtpserver` and `pop3server` manually if you like, but they will require root as they bind to low port numbers.
For `smtprelay`, you will need to run `smtprelay listen` and `smtprelay send` as seperate processes.

## Docker compose

You can use the docker compose file provided, or write your own if you like. The provided `compose.yaml` is designed to go in the parent directory of this git repository, and creates services for the pop and smtp servers, as well as a relay.

## Accounts

If you want to use the pop3server, you will need to create an account. Install sqlite3 and use `sqlite3 db_path` to connect to the mail database. Once you are connected, you can run `INSERT INTO users (email_address,password) VALUES ("<your email here>","<your hashed password>");`. It uses sha256 as the hashing algorithm, so you could use `echo -n "mypass" | sha256sum` to generate the hashed password value. Once you have added an account, you can see the accounts using `SELECT * from users;`

## Manually viewing mail

Mail is stored in the `emails` table, so you can use `sqlite3` to query them manually or create a python script to retrieve emails for you.

# smtpserver

smtpserver is a server for receiving emails over SMTP. It is not a relay. It stores the received emails at an sqlite database `/var/mail/mail.db` in a format that pop3server recognises. It does not currently support TLS so traffic is unencrypted. The whole idea behind this server is that it has one global inbox, that all users can access, meaning that if you need access to a lot of different email addresses, you can simply log in with any account and view emails to any address. It does not enforce mailboxes, so even if a user doesn't exist, it will still allow the email to be delivered.

# pop3server

pop3server is a server for accessing mail stored by the smtpserver program. It provides basic features such as mail fetching and deletion. It is designed to be a way to access and manage mail stored by smtpserver to facilitate using graphical applications such as thunderbird. Does not support TLS currently, so it is not recommended to port forward this.

# smtprelay

smtprelay is a relay server that accepts outbound mail on port `9185` and forwards it to the correct destination. It operates in 2 different modes, with a listen and a send mode. When in listening mode, it will recieve emails on port `9185` and queue them at `/var/mail/outbound_queue.db`. when operating in send mode, it attempts to send all the queued emails stored in the database, and delete them once they have been delivered. For running it, you will need to run 2 processes with one running `smtprelay listen` and one running `smtprelay send`.

# Accessing mail store programmatically

## smtpserver database ERD

```
+emails---------------------+
| id INTEGER PK             | 
| receipt_timestamp INTEGER |-----------------------+
| senders TEXT              |                      /|\
| recipients TEXT           |          +received-------------------------+
| data TEXT                 |          | email_id INTEGER FK (emails id) |
+---------------------------+          | user_id INTEGER FK (users id)   |
                                       +---------------------------------+
+users---------------+                             \|/
| id INTEGER PK      |                              |
| email_address TEXT |------------------------------+
| password TEXT      | 
+--------------------+
```
Notes:
 - received is not currently set up to have appropriate records added
 - foreign keys are not set to be enforced
 - `data` is stored according to RFC 5322 (with any trailing `<CRLF>` stripped)

## smtprelay database ERD

```
+emails---------+
| id INTEGER PK |
| senders TEXT  |
| data TEXT     |
+---------------+
     |
     |
    /|\
+recipient_queue------------------+
| recipient TEXT                  | 
| email_id INTEGER FK (emails id) |
| time_added INTEGER              |
+---------------------------------+
```
Notes:
 - `time_added` stored as UNIX timestamp
 - foreign keys are enforced
 - `email_id` set to `ON DELETE RESTRICT` so an email cannot be deleted if a recipient is queued for it
