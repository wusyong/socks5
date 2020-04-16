use std::collections::HashMap;
use std::io::{self, Result, Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, SocketAddr};

use mio::net::{TcpListener, TcpStream};
use mio::{Token, Events, Poll, Interest};

const SERVER: Token = Token(0);

enum Event {
    Request(TcpStream),
    Proxy(TcpStream, TcpStream),
}

pub struct Server {
    socks: TcpListener,
    poll: Poll,
    events: HashMap<Token, Event>,
}

impl Server {
    pub fn new() -> Self {
        Server {
            socks: TcpListener::bind("0.0.0.0:4443".parse().unwrap()).unwrap(),
            poll: Poll::new().unwrap(),
            events: HashMap::new(),
        }
    }

    pub fn run(&mut self) -> Result<()> {
        self.poll.registry().register(&mut self.socks, SERVER, Interest::READABLE)?;
        let mut events = Events::with_capacity(1024);

        loop {
            self.poll.poll(&mut events, None)?;
            for event in &events {
                let token = event.token();

                if token == SERVER && event.is_readable() {
                    if let Ok((sock, _)) = self.socks.accept() {
                        let mut sock = sock;
                        let new_token = Token(self.events.len() + 1);
                        self.poll.registry().register(&mut sock, new_token, Interest::READABLE | Interest::WRITABLE)?;
                        self.events.insert(new_token, Event::Request(sock));
                    };
                    continue;
                }

                if event.is_readable() && event.is_writable() {
                    if let Some(event) = self.events.remove(&token) {
                        match event {
                            Event::Request(mut stream) => {
                                Server::read_version(&mut stream)?;
                                let method = Server::read_methods(&mut stream)?;
                                Server::write_ack(&mut stream, method)?;

                                Server::read_version(&mut stream)?;
                                Server::read_command(&mut stream)?;
                                Server::read_reserved(&mut stream)?;
                                let address = Server::read_address(&mut stream)?;
                                let dst = Server::write_reply(&mut stream, address)?;

                                self.events.insert(token, Event::Proxy(stream, dst));
                            },
                            Event::Proxy(mut client, mut dst) => {
                                let mut buf = Vec::new();
                                let c = client.read_to_end(&mut buf)?;
                                dst.write_all(&mut buf)?;
                                let d =dst.read_to_end(&mut buf)?;
                                client.write_all(&mut buf)?;
                                
                                if c == 0 && d == 0 {
                                    self.poll.registry().deregister(&mut client)?;
                                } else {
                                    self.events.insert(token, Event::Proxy(client, dst));
                                }
                            },
                        }
                    }
                }
            }
        }
    }

    pub fn read_version<T: Read>(stream: &mut T) -> Result<()> {
        if read_byte(stream)? != 5 {
            return Err(io::Error::new(io::ErrorKind::Other, "Version no supported"));
        }

        Ok(())
    }

    pub fn read_methods<T: Read>(stream: &mut T) -> Result<u8> {
        let n = read_byte(stream)?;
        let mut methods = vec![0u8; n as usize];
        
        stream.read(&mut methods)?;
        // TODO choose method
        if methods[0] != 0 {
            return Err(io::Error::new(io::ErrorKind::Other, "Method not supported"));
        }

        Ok(0)
    }

    pub fn read_command<T: Read>(stream: &mut T) -> Result<()> {
        if read_byte(stream)? != 1 {
            return Err(io::Error::new(io::ErrorKind::Other, "Command no supported"));
        }

        Ok(())
    }

    pub fn read_reserved<T: Read>(stream: &mut T) -> Result<()> {
        read_byte(stream)?;

        Ok(())
    }

    pub fn read_address<T: Read>(stream: &mut T) -> Result<SocketAddr> {
        let mut atyp = match read_byte(stream)? {
            1 => [0u8; 6],
            _ => return Err(io::Error::new(io::ErrorKind::Other, "Address type no supported")),
        };

        stream.read(&mut atyp[..])?;
        let socket = SocketAddrV4::new(
            Ipv4Addr::new(atyp[0], atyp[1], atyp[2], atyp[3]),
            ((atyp[4] as u16) << 8) | atyp[5] as u16,
        );
        
        Ok(SocketAddr::V4(socket))
    }

    pub fn write_ack<T: Write>(stream: &mut T, method: u8) -> Result<()> {
        let ack = [5, method];
        stream.write(&ack)?;

        Ok(())
    }

    pub fn write_reply<T: Write>(stream: &mut T, address: SocketAddr) -> Result<TcpStream> {
        let dst = TcpStream::connect(address)?;
        let mut res = Vec::new();
        // VER
        res.push(5);
        // REP
        res.push(0);
        // RSV
        res.push(0);
        match dst.local_addr()? {
            SocketAddr::V4(addr) => {
                res.push(1);
                res.extend(addr.ip().octets().iter());
                res.push((addr.port() >> 8) as u8);
                res.push(addr.port() as u8);
            },
            _ => return Err(io::Error::new(io::ErrorKind::Other, "Address type no supported")),
        }
        stream.write(&res)?;

        Ok(dst)
    }
}

fn read_byte<T: Read>(stream: &mut T) -> Result<u8> {
    let mut byte = [0];
    stream.read(&mut byte)?;
    Ok(byte[0])
}