fn main() {
    let config = Config::new("test", 8080);
    run(&config);
}

fn run(config: &Config) {
    println!("Running on port {}", config.port);
}

struct Config {
    name: String,
    port: u16,
}

impl Config {
    fn new(name: &str, port: u16) -> Self {
        Config {
            name: name.to_string(),
            port,
        }
    }
}

trait Handler {
    fn handle(&self, input: &str) -> String;
}

struct EchoHandler;

impl Handler for EchoHandler {
    fn handle(&self, input: &str) -> String {
        input.to_string()
    }
}

fn process(handler: &dyn Handler, input: &str) -> String {
    handler.handle(input)
}
