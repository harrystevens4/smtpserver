pub struct Email {
	sender_address: String,
	recipient_address: String,
	body: String,
}

impl Default for Email {
	fn default() -> Self {
		Email {
			sender_address: "anonymous".into(),
			recipient_address: "anonymous".into(),
			body: String::new(),
		}
	}
}
