use maildb::Email;

use std::time::{Instant};
use std::convert;

pub struct QueuedEmail {
	email: Email,
	time_added: Instant
}

pub struct EmailQueue {
	queue: Vec<QueuedEmail>,
}

impl EmailQueue {
	pub fn new() -> Self {
		EmailQueue {
			queue: vec![],
		}
	}
	pub fn enqueue(&mut self, email: QueuedEmail){
		self.queue.push(email);
	}
	pub fn dequeue(&mut self) -> QueuedEmail {self.queue.remove(0)}
	pub fn len(&self) -> usize {self.queue.len()}
}

impl QueuedEmail {
	pub fn email<'a>(&'a self) -> &'a Email {&self.email}
}
impl From<Email> for QueuedEmail {
	fn from(email: Email) -> Self {
		QueuedEmail {email, time_added: Instant::now()}
	}
}
