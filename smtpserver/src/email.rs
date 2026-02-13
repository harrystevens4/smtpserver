use std::time::{SystemTime,UNIX_EPOCH,Duration};

pub struct Email {
	pub senders: Vec<String>,
	pub recipients: Vec<String>,
	pub body: String,
	pub timestamp: u64,
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
			body: String::new(),
			timestamp,
		}
	}
}

impl Email {
	pub fn timestamp(&self) -> u64 {self.timestamp}
	pub fn body(&self) -> String {self.body.clone()}
	pub fn senders_string(&self) -> String {
		let senders_string = String::new();
		self.senders
			.clone()
			.into_iter()
			.map(|sender| sender + ";") //semi colon seperated senders
			.fold(String::new(),|senders,sender| senders + &sender)
	}
	pub fn recipients_string(&self) -> String {
		let recipients_string = String::new();
		self.recipients
			.clone()
			.into_iter()
			.map(|recipient| recipient + ";") //semi colon seperated recipients
			.fold(String::new(),|recipients,recipient| recipients + &recipient)
	}
}
