use std::{
    io::{
        Read,
        Write,
        Error,
        Result,
        ErrorKind
    },
    net::{
        TcpListener,
        TcpStream,
        Shutdown
    }
};
use byteorder::{
    LE,
    WriteBytesExt,
};

use bson::Document;
use dns_lookup::lookup_host;

const ENABLE_LOGGING: bool = true;
const ENABLE_PING_LOGGING: bool = true;
static mut CURRENT_IP: String = String::new();

fn encode_bson(document: &mut Document, vec: &mut Vec<u8>) -> Result<()> {
    let dump = bson::to_vec(&document).unwrap();

    vec.clear();
    vec.write_u32::<LE>(dump.len() as u32 + 4)?;
    vec.write_all(&dump)?;

    Ok(())
}

fn send(from: &mut TcpStream, to: &mut TcpStream, from_server: bool) -> Result<()> {
    let mut buffer = [0; 65535];

    match from.read(&mut buffer) {
        Ok(buffer_size) => {
            if buffer_size == 0 {
                to.shutdown(Shutdown::Both)?;
                return Err(Error::new(ErrorKind::Other, "Disconnected!"));
            }

            if !from_server && buffer_size > 1024 {
                panic!("Client send packet with size more than 1024!");
            }

            let received_data = &mut buffer[..buffer_size].to_vec();
            let mut received_data_bson = &received_data[4..];

            match Document::from_reader(&mut received_data_bson) {
                Ok(mut document) => {
                    let message_count = document.get_i32("mc").unwrap();
                    let mut ignore_packet = false;

                    for i in 0..message_count {
                        let message_document = document.get_document_mut(format!("m{}", i)).unwrap();
                        let message_id = message_document.get_str("ID").unwrap();

                        if let Ok(player_data) = message_document.get_binary_generic("pD") {
                            match Document::from_reader(&mut player_data.as_slice()) {
                                Ok(doc) => {
                                    println!("Player data: {}", doc);    
                                    if let Ok(inv) = doc.get_binary_generic("inv") {
                                        for i in inv.iter() {
                                            print!("{:02x}", i);
                                        }
                                        println!("");
                                    }
                                }

                                _ => {}
                            }
                            // for i in player_data.iter() {
                            //     print!("{:02x}", i);
                            // }
                            // println!("");
                        }

                        match message_id {
                            "OoIP" => { // subserver switching
                                let ip = message_document.get_str("IP").unwrap();
                                match lookup_host(ip) {
                                    Ok(ips) => {
                                        if ips.len() > 0 {
                                            let first = ips.first().unwrap();

                                            unsafe {
                                                CURRENT_IP = first.to_string().to_owned();
                                                if CURRENT_IP == "127.0.0.1" {
                                                    CURRENT_IP = String::from("44.194.163.69");
                                                }

                                                println!("Connecting to {}", CURRENT_IP);

                                                match connect(CURRENT_IP.as_str()) {
                                                    Ok(stream) => {
                                                        *from = stream;

                                                        if message_document.insert("IP", "prod.gamev80.portalworldsgame.com").is_none() {
                                                            println!("Error setting ID to prod.gamev80.portalworldsgame.com!");
                                                        }
                                                        else if encode_bson(&mut document, received_data).is_err() {
                                                            println!("Error encoding bson!");
                                                        }
                                                    },
                            
                                                    Err(e) => println!("Failed to redirect to pixel world server! Error: {}", e)
                                                }
                                            }
                                        } else {
                                            println!("Ips empty?? sus.. ({})", ip);
                                        }
                                    },

                                    Err(e) => {
                                        println!("Failed to lookup host of {}", ip);
                                        return Err(e);
                                    }
                                }
                            },
                            
                            "GPd" => {
                                //println!("{:?}", received_data);
                                
                            }

                            "ST" => {
                                ignore_packet = true;
                            },

                            "WCM" => {
                                if let Ok(msg) = message_document.get_str("msg") {
                                    if msg == "!test" {
                                        println!("Works!");
                                    }
                                }
                            }

                            "p" | "mP" => {
                                if message_count == 1 && message_document.len() == 1 {
                                    ignore_packet = true;
                                }
                            }

                            _ => {}
                        }

                        
                    }

                    if message_count > 0 {
                        if ENABLE_LOGGING {
                            if !ignore_packet || ENABLE_PING_LOGGING {
                                let identifier = if from_server {
                                    "server"
                                } else {
                                    "client"
                                };
                    
                                println!("Received from {}: {}", identifier, document);
                            }
                        }
                    }
                },

                Err(_) => println!("Error reading document!"),
            }

            match to.write(&received_data) {
                Ok(_) => {
                    to.flush()?;

                    Ok(())
                },

                Err(e) => Err(e)
            }
        },
        Err(e) => Err(e)
    }
}

fn connect(ip: &str) -> Result<TcpStream> {
    match TcpStream::connect(format!("{}:10001", ip)) {
        Ok(stream) => {
            println!("Successfully connected to pixel world server!");

            Ok(stream)
        },

        Err(e) => Err(e)
    }
}

fn main() {
    unsafe {
        CURRENT_IP = String::from("44.194.163.69");
    };

    let proxy_server = TcpListener::bind("0.0.0.0:10001").unwrap();

    let mut connected = false;
    let mut pw_client: Option<TcpStream> = None;
    let mut proxy_peer: Option<TcpStream> = None;

    loop {
        if !connected {
            match proxy_server.accept() {
                Ok((stream, addr)) => {
                    proxy_peer = Some(stream);
                    
                    println!("Peer connected to proxy with address: {:?}", addr);

                    unsafe {
                        match connect(CURRENT_IP.as_str()) {
                            Ok(stream) => {
                                connected = true;
                                pw_client = Some(stream);
                            },

                            Err(e) => println!("Failed to connect to pixel world server! Error: {}", e)
                        }
                    }
                },

                Err(e) => println!("Peer failed to connect! Error: {}", e)
            }
        } else {
            if let Some(peer1) = &mut proxy_peer {
                if let Some(peer2) = &mut pw_client {
                    if send(peer1, peer2, false).is_err() || send(peer2, peer1, true).is_err() {
                        connected = false;

                        unsafe {
                            CURRENT_IP = String::from("44.194.163.69");
                        };
                    }
                }
            }
        }
    }
}
