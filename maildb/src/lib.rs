use std::time::{SystemTime,UNIX_EPOCH,Duration};
use rusqlite::{Connection,Error as sqlError,params};

#[derive(Clone,Debug)]
pub struct Email {
	pub senders: Vec<String>,
	pub recipients: Vec<String>,
	pub data: String,
	pub timestamp: u64,
	id: usize,
}

impl Default for Email {
	fn default() -> Self {
		let timestamp = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.unwrap_or(Duration::default())
			.as_secs();
		Email {
			senders: vec![],
			recipients: vec![],
			data: String::new(),
			timestamp,
			id: 0,
		}
	}
}

impl Email {
	pub fn timestamp(&self) -> u64 {self.timestamp}
	pub fn data(&self) -> String {self.data.clone()}
	pub fn id(&self) -> usize {self.id}
	pub fn senders_vec(&self) -> Vec<String> {self.senders.clone()}
	pub fn recipients_vec(&self) -> Vec<String> {self.recipients.clone()}
	pub fn senders_string(&self) -> String {
		self.senders
			.clone()
			.into_iter()
			.map(|sender| sender + ";") //semi colon seperated senders
			.fold(String::new(),|senders,sender| senders + &sender)
	}
	pub fn recipients_string(&self) -> String {
		self.recipients
			.clone()
			.into_iter()
			.map(|recipient| recipient + ";") //semi colon seperated recipients
			.fold(String::new(),|recipients,recipient| recipients + &recipient)
	}
}

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
			data TEXT
		)
		",[])?;
		//users table
		db_connection.execute("
		CREATE TABLE IF NOT EXISTS users (
			id INTEGER PRIMARY KEY,
			email_address TEXT,
			password TEXT
		)
		",[])?;
		//received table (intermediate between users and emails)
		db_connection.execute("
		CREATE TABLE IF NOT EXISTS received (
			email_id INTEGER,
			user_id INTEGER,
			FOREIGN KEY (email_id) REFERENCES emails (id)
				ON DELETE CASCADE
				ON UPDATE CASCADE,
			FOREIGN KEY (user_id) REFERENCES users (id)
				ON DELETE CASCADE
				ON UPDATE CASCADE,
			PRIMARY KEY (email_id,user_id)
		)
		",[])?;
		//construct the db struct
		Ok(MailDB {
			db: db_connection,
		})
	}
	pub fn store_email(&self, email: Email) -> Result<(),sqlError> {
		self.db.execute("
		INSERT INTO emails (receipt_timestamp, senders, recipients, data)
		VALUES (?, ?, ?, ?)
		",params![
			email.timestamp() as i64,
			email.senders_string(),
			email.recipients_string(),
			email.data()
		])?;
		Ok(())
	}
	pub fn check_user_exists(&self, username: &str) -> Result<bool,sqlError> {
		self.db.query_row("
			SELECT 1
			FROM users
			WHERE email_address = ?
		",[username], |row| row.get(0).map(bool::from))
	}
	pub fn verify_user_password(&self, username: &str, password: &str) -> Result<bool,sqlError> {
		self.db.query_row("
			SELECT password
			FROM users
			WHERE email_address = ?
		",[username], |row|{
			let retrieved_password: String = row.get(0)?;
			Ok(sha256::digest(password) == retrieved_password)
		})
	}
	pub fn retrieve_mail(&self, username: &str) -> Result<Vec<Email>,sqlError> {
		let mut statement = self.db.prepare("
			SELECT senders, recipients, data, receipt_timestamp, id
			FROM emails
		")?;
		statement
			.query_map([],|row|{
				let mut email = Email::default();
				email.senders = row.get::<_,String>(0)?
					.split(';')
					.map(String::from)
					.collect();
				email.recipients = row.get::<_,String>(1)?
					.split(';')
					.map(String::from)
					.collect();
				email.data = row.get(2)?;
				email.timestamp = row.get::<_,i64>(3)? as u64;
				email.id = row.get::<usize,i64>(4)? as usize;
				Ok(email)
			})?
			.collect()
	}
	pub fn delete_email(&self, email_id: usize) -> Result<(),sqlError> {
		self.db.execute("
			DELETE FROM emails
			WHERE id = ?
		",params![
			email_id as i64
		])?;
		Ok(())
	}
}
