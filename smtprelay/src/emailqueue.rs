use maildb::Email;

use std::time::{SystemTime,UNIX_EPOCH,Duration};
use std::convert::{From,Into};
use std::path::Path;
use rusqlite::{Connection,params,Error as SQLError,OptionalExtension};
use std::error::Error;
use std::io;

#[derive(Debug)]
pub struct QueuedEmail {
	email: Email,
	time_queued: SystemTime,
	id: Option<i64>,
}

#[derive(Debug)]
pub struct EmailQueue {
	database: Connection,
}

impl EmailQueue {
	pub fn new<P: AsRef<Path>>(db_path: P) -> Result<Self,SQLError> {
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
				data TEXT
			)
		",[])?;
		//one email may have many recipients, so reuse the email
		database.execute("
			CREATE TABLE IF NOT EXISTS recipient_queue (
				queue_id INTEGER PRIMARY KEY,
				recipient TEXT,
				email_id INTEGER,
				time_added INTEGER,
				attempts INTEGER,
				FOREIGN KEY (email_id) REFERENCES emails (id)
					ON UPDATE CASCADE
					ON DELETE RESTRICT
			)
		",[])?;
		//===== enable foreign key constraints ======
		database.execute("PRAGMA foreign_keys = ON",[])?;
		//====== init the struct ======
		Ok(EmailQueue {
			database,
		})
	}
	pub fn enqueue<E: Into<QueuedEmail>>(&self, email: E) -> Result<(),Box<dyn Error>>{
		let queued_email = email.into();
		let email = queued_email.email();
		//add email to database
		self.database.execute("
			INSERT INTO emails (senders,data)
			VALUES (?,?)
		",[email.senders_string(),email.data()])?;
		let email_id = self.database.last_insert_rowid();
		//add each recipient to queue
		for recipient in email.recipients_vec() {
			self.database.execute("
				INSERT INTO recipient_queue (recipient,email_id,time_added,attempts)
				VALUES (?,?,?,0)
			",params![
				recipient,
				email_id,
				SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64,
			])?;
		}
		Ok(())
	}
	pub fn peek(&self) -> Result<Option<QueuedEmail>,SQLError> {
		//====== fetch lowest attempts in queue ======
		//by fetching lowest attempts, it will make sure to evenly 
		//select not only the first in the queue but messages that havent
		//been attempted yet
		let Some((email_id,recipient,time_added)): Option<(i64,String,i64)> = self.database.query_row("
			SELECT email_id,recipient,time_added
			FROM recipient_queue
			ORDER BY attempts ASC
			LIMIT 1
		",[],|row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional()?
		else {
			//nothing left in the queue
			return Ok(None)
		};
		//====== fetch corresponding email ======
		let (senders,data): (String,String) = self.database.query_one("
			SELECT senders,data
			FROM emails
			WHERE id = ?
		",[email_id],|row| Ok((row.get(0)?,row.get(1)?)))?;
		let senders_vec = senders
			.trim_end_matches(';')
			.split(';')
			.into_iter()
			.map(String::from)
			.collect();
		//====== construct QueuedEmail ======
		let email = Email::new(senders_vec,vec![recipient],data);
		let mut queued_email = QueuedEmail::from(email);
		queued_email.id = Some(email_id);
		queued_email.time_queued = UNIX_EPOCH + Duration::new(time_added as u64,0);
		Ok(Some(queued_email))
	}
	pub fn retry_later(&self, email: QueuedEmail) -> Result<(),Box<dyn Error>>{
		//check email has valid id
		let Some(id) = email.id() else {
			return Err(io::Error::other("QueuedEmail does not originate from queue"))?
		};
		//increment attempts
		self.database.execute("
			UPDATE recipient_queue
			SET attempts = attempts + 1
			WHERE recipient = ?
		",[id])?;
		Ok(())
	}
	pub fn delete(&self, email: QueuedEmail) -> Result<(),Box<dyn Error>>{
		let Some(id) = email.id() else {
			return Err(io::Error::other("QueuedEmail does not originate from queue"))?
		};
		let Some(recipient) = email.email().recipients_vec().pop() else {
			return Err(io::Error::other("Email has no recipients"))?;
		};
		//====== delete recipient from queue ======
		self.database.execute("
			DELETE FROM recipient_queue
			WHERE recipient = ?
		",[recipient])?;
		//====== attempt to delete email ======
		//will fail if there are still recipients in queue with this email
		let _ = self.database.execute("
			DELETE FROM emails
			WHERE id = ?
		",[id]);
		Ok(())
	}
}

impl QueuedEmail {
	pub fn new(email: Email, time_queued: SystemTime) -> Self { 
		QueuedEmail {
			email,time_queued,id: None
		}
	}
	pub fn email<'a>(&'a self) -> &'a Email {&self.email}
	pub fn id(&self) -> Option<i64> {self.id}
	pub fn time_queued(&self) -> SystemTime {self.time_queued.clone()}
}
impl From<Email> for QueuedEmail {
	fn from(email: Email) -> Self {
		QueuedEmail {email, time_queued: SystemTime::now(), id: None}
	}
}
