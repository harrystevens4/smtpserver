use std::env;

#[derive(Debug)]
pub struct Args {
	options: Vec<(char,Option<String>)>, //(short_opt,Option<argument>)
	others: Vec<String>,
}

impl Args {
	//[(short,long,argument_required)]
	pub fn gather(option_config: &[(char,Option<&str>,bool)]) -> Args {
		let mut args: Vec<_> = env::args().rev().collect();
		let _ = args.pop(); //remove argv[0]
		let mut other_args = vec![];
		let mut option_args = vec![];
		loop {
			let Some(arg) = args.pop() else {break};
			//stop processing further options
			if arg == "--" {break}
			//====== long options ======
			if arg.len() > 2 && &arg[..2] == "--" {
				let option = &arg[2..];
				if let Some((short,_,argument)) = option_config.iter().find(|(_,long,_)| *long == Some(option)){
					//has argument?
					let value = if *argument {
						Some(args.pop().unwrap_or(String::from("")))
					}else {None};
					option_args.push((*short,value));
				}else{
					eprintln!("Unrecognised long option: {arg}");
				}
			}
			//====== short options ======
			else if arg.len() > 1 && &arg[..1] == "-" {
				for short_arg in arg[1..].chars() {
					if let Some((_,_,argument)) = option_config.iter().find(|(short,_,_)| *short == short_arg){
						//argument or no
						let value = if *argument {
							Some(args.pop().unwrap_or(String::from("")))
						}else {None};
						option_args.push((short_arg,value))
					}else {
						eprintln!("Unrecognised short option: {short_arg}");
					}
				}
			}else {
				other_args.push(arg);
			}
		}
		//add any remaining args to other
		other_args.append(&mut args);
		Args{
			options: option_args,
			others: other_args,
		}
	}
}
