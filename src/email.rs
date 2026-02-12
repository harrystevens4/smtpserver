pub struct Email {
	pub senders: Vec<String>,
	pub recipients: Vec<String>,
	pub body: String,
}

impl Default for Email {
	fn default() -> Self {
		Email {
			senders: vec![],
			recipients: vec![],
			body: String::new(),
		}
	}
}
