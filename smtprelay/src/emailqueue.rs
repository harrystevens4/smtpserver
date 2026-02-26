use maildb::Email;

use std::time::{Instant,SystemTime,UNIX_EPOCH};
use std::convert::{From,Into};
use std::path::Path;
use rustqlite::{Connection,params,Error as SQLError}
use std::error::Error;

#[derive(Debug)]
pub struct QueuedEmail {
	email: Email,
	time_added: Instant,
	id: i64,
}

#[derive(Debug)]
pub struct EmailQueue {
	queue: Vec<QueuedEmail>,
	database: Connection,
}

impl EmailQueue {
	pub fn new<P: AsRef<&Path>>(db_path: P) -> Result<Self,SQLError> {
		//====== connect to the database ======
		let database = Connection::open(db_path.as_ref())?;
		//====== create relevant tables ======
		//+emails------+       +recipient_queue---+
		//|PK id       |      /|   recipient      |
		//|   senders  |-----+-|FK email_id       |
		//|   data     |      \|   time_added     |
		//+------------+       +------------------+
		database.execute("
			CREATE TABLE IF NOT EXISTS emails (
				id INTEGER PRIMARY KEY,
				senders TEXT,
				data TEXT,
			)
		",[])?;
		//one email may have many recipients, so reuse the email
		database.execute("
			CREATE TABLE IF NOT EXISTS recipient_queue (
				recipient TEXT PRIMARY KEY,
				email_id INTEGER,
				time_added INTEGER,
				FOREIGN KEY (email_id) REFERENCES emails (id)
					ON UPDATE CASCADE
					ON DELETE RESTRICT
			)
		",[])?;
		//====== init the struct ======
		EmailQueue {
			queue: vec![],
			database,
		}
	}
	pub fn enqueue<E: Into<QueuedEmail>>(&mut self, email: E) -> Result<(),Box<dyn Error>>{
		let queued_email = email.into<QueuedEmail>();
		let email = queued_email.email();
		//add email to database
		self.database.execute("
			INSERT INTO emails (senders,data)
			VALUES (?,?)
		",[email.senders_string(),email.data()])?;
		email_id = database.last_insert_rowid();
		//add each recipient to queue
		for recipient in recipients_vec {
			self.database.execute("
				INSERT INTO recipient_queue (recipient,email_id,time_added)
				VALUES (?,?,?)
			",[
				recipient,
				email_id,
				SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
			])?;
		}
	}
	pub fn peek(&mut self) -> Option<QueuedEmail> {
		//====== fetch oldest in queue ======
		let (email_id,recipient,time_added): (i64,String,i64) = self.database.query_one("
			SELECT email_id,recipient,time_added
			FROM recipient_queue
			ORDER BY time_added ASC
			LIMIT 1
		",[],|row| (row.get(0)?,row.get(1)?,row.get(2)?));
		//====== fetch corresponding email ======
		let (senders,data) = self.database.query_one("
			SELECT senders,data
			FROM emails
			WHERE id = ?
		",[email_id])
		//====== construct QueuedEmail ======
		let email = Email::new(senders,vec![recipient],data);
		let queued_email = QueuedEmail::new();
	}
	pub fn delete(&mut self, email: QueuedEmail) -> QueuedEmail {
		let id = email.id();
		//====== delete recipient from queue ======
		self.database.execute("
			DELETE FROM recipient_queue
			WHERE id = ?
		",[id])?;
		//====== attempt to delete email ======
		//will fail if there are still recipients in queue with this email
		let _ = self.database.execute("
			DELETE FROM emails
			WHERE id = ?
		",[id]);
	}
}

impl QueuedEmail {
	pub fn new(email: Email, time_added: Instant, id: i64) -> Self { QueuedEmail {email,time_added,id} }
	pub fn email<'a>(&'a self) -> &'a Email {&self.email}
	pub fn id(&self) -> i64 {self.id};
}
impl From<Email> for QueuedEmail {
	fn from(email: Email) -> Self {
		QueuedEmail {email, time_added: Instant::now(), id: 0}
	}
}
