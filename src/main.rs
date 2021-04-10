use async_std::task;
use async_trait::async_trait;
use async_std::channel::{unbounded};
use broadway::actor::*;

#[async_std::main]
async fn main() {
    println!("Hello, world!");
    let (sender, instance) = ActorInstance::<dyn Greeter>::start();
    let mut i: i128 = 0;
    loop{
        let mut sender = sender.clone();
        if i % 100 == 0{
            task::spawn(async move {sender.set_name(i.to_string()).await});
        }
        else{
            task::spawn(async move {println!("{}", sender.greet().await)});
        }
        i += 1;
        // This demo still works without this, but you'll see much longer stretches of names
        // WHICH IS FINE, it's just clearing out mutable calls and building up slow println calls.
        //task::sleep(std::time::Duration::from_millis(1)).await;
    }
}

// EVERYTHING FROM HERE TO THE NEXT COMMENT IS PRETTY MUCH
// ALL AN END USER SHOULD NEED TO WRITE FOR AN ACTOR, EVERYTHING ELSE
// SHOULD/COULD BE A LIBRARY ITEM OR MACRO GENERATED
#[async_trait]
pub trait Greeter{
    async fn set_name(&mut self, name: String);

    async fn greet(&self) -> String;
}

pub struct GreeterActor{
    name: String,
}

#[async_trait]
impl Greeter for GreeterActor{
    async fn set_name(&mut self, name: String){
        self.name = name;
    }

    async fn greet(&self) -> String{
        format!("Hello! My name is {}", self.name)
    }
}

impl Actor for GreeterActor{
    fn new() -> Self{
        GreeterActor{
            name: "James".to_string()
        }
    }
}

// IDEALLY THE END USER WOULD HAVE A MACRO THAT LOOKS LIKE THIS:
// #[role(GreeterActor)]
// That they would put over 'pub trait Greeter'
// that would generate everything below this comment.
impl<'a> Role for dyn Greeter + 'a{
    type Actor = GreeterActor;

    type Calls = Call<GreeterCall, GreeterReply>;
    type MutCalls = Call<GreeterMutCall, GreeterReply>;
}

pub enum GreeterReply{
    Greet(String),
    SetName(()),
}

pub enum GreeterCall{
    Greet,
}


#[async_trait]
impl Handler<GreeterActor> for Call<GreeterCall, GreeterReply>{
    async fn handle(self, actor: &GreeterActor){
        match self.call{
            GreeterCall::Greet => self.return_channel.send(GreeterReply::Greet(GreeterActor::greet(actor).await)).await,
        };
    }
}

pub enum GreeterMutCall{
    SetName(String)
}

#[async_trait]
impl MutHandler<GreeterActor> for Call<GreeterMutCall, GreeterReply>{
    async fn handle_mut(self, actor: &mut GreeterActor){
        match self.call{
            GreeterMutCall::SetName(name) => self.return_channel.send(GreeterReply::SetName(GreeterActor::set_name(actor, name).await)).await,
        };
    }
}

#[async_trait]
impl Greeter for ActorChannel<dyn Greeter>{
    async fn set_name(&mut self, name: std::string::String) {
        let (return_channel, receiver) = unbounded();
        self.mut_calls_sender.send(
            Call{
                return_channel,
                call: GreeterMutCall::SetName(name)
            }
        ).await;
        receiver.recv().await.unwrap();
    }

    async fn greet(&self) -> String { 
        let (return_channel, receiver) = unbounded();
        self.calls_sender.send(
            Call{
                return_channel,
                call: GreeterCall::Greet
            }
        ).await;
        if let Ok(GreeterReply::Greet(greeting)) = receiver.recv().await{
            greeting
        }
        else{
            panic!("Ruh roh");
        }
    }
}