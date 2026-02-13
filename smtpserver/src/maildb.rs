use rusqlite::{Connection,Error as sqlError,params};
use crate::email::Email;

pub struct MailDB {
	db: Connection,
}

impl MailDB {
	pub fn open(db_location: &str) -> Result<Self,sqlError> {
		let db_connection = Connection::open(db_location)?;
		//====== create relevant tables if they dont exist ======
		//emails table
		db_connection.execute("
		CREATE TABLE IF NOT EXISTS emails (
			id INTEGER PRIMARY KEY,
			receipt_timestamp INTEGER,
			senders TEXT,
			recipients TEXT,
			body TEXT
		)
		",[])?;
		//users table
		db_connection.execute("
		CREATE TABLE IF NOT EXISTS users (
			id INTEGER PRIMARY KEY,
			name TEXT
		)
		",[])?;
		//construct the db struct
		Ok(MailDB {
			db: db_connection,
		})
	}
	pub fn store_email(&self, email: Email) -> Result<(),sqlError> {
		self.db.execute("
		INSERT INTO emails (receipt_timestamp, senders, recipients, body)
		VALUES (?, ?, ?, ?)
		",params![
			email.timestamp() as i64,
			email.senders_string(),
			email.recipients_string(),
			email.body()
		])?;
		Ok(())
	}
}
