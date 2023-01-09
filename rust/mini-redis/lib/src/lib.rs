//! # Layout
//!
//! Los componentes principales son:
//!
//! * `server`: Implementacion de un servidor Redis. Incluye una funcion simple
//!   'run' que tiene como parametro un 'TcpListener' mediante el cual
//!   aceptara conexiones de clientes.
//!
//! * `client`: La implementacion de un cliente asincrono Redis. Demuestar como
//!   construir clientes con Tokio.
//!
//! * `cmd`: Implementacion de los comandos redis soportados.
//!
//! * `frame`: Representa una trama simple del protocolo Redis. Una trama
//!   es utilizada como una representacion intermedia entre un 'comandos
//!   y su representacion en bytes.

pub mod blocking_client;
pub mod client;

pub mod cmd;
pub use cmd::Command;

mod connection;
pub use connection::Connection;

pub mod frame;
pub use frame::Frame;

mod db;
use db::Db;
use db::DbDropGuard;

mod parse;
use parse::{Parse, ParseError};

pub mod server;

mod buffer;
pub use buffer::{buffer, Buffer};

mod shutdown;
use shutdown::Shutdown;

/// Puerto por defecto por el que el servidor redis escuchara (se utilizara
/// si no se especifica ninguno)
pub const DEFAULT_PORT: u16 = 6379;

/// Error returnado por la mayoria de funciones.
/// 
/// Al escribir una aplicación real, es posible que desee considerar una crate
/// de manejo de errores especializado o definir un tipo de error como 
/// una "enumeración" de causas.
/// 
/// Sin embargo, para nuestro ejemplo, usar un Boxed `std::error::Error` 
/// es suficiente.
/// 
/// Por motivos de rendimiento, se evita el boxing en cualquier ruta activa.
/// Por ejemplo, en `parse`, se define un error personalizado `enum`.
/// Esto se debe a que el error se detecta y maneja durante la ejecución
/// normal cuando se recibe un marco parcial en un socket.
/// 
/// `std::error::Error` se implementa para `parse::Error`, 
/// lo que permite convertirlo en `Box<dyn std::error::Error>`.
pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// Un tipo 'Result' especializado para las operaciones Redis.
pub type Result<T> = std::result::Result<T, Error>;
