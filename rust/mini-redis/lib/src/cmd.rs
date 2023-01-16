mod get;
pub use get::Get;

mod publish;
pub use publish::Publish;

mod set;
pub use set::Set;

mod subscribe;
pub use subscribe::{Subscribe, Unsubscribe};

mod ping;
pub use ping::Ping;

mod unknown;
pub use unknown::Unknown;

use crate::{Connection, Db, Frame, Parse, ParseError, Shutdown};

/// Enumeraciopn de los comandos Redis que son soportados.
///
/// Metodos  llamados en 'Command' son delegados a la implementaciuon
/// del comandos.
#[derive(Debug)]
pub enum Command {
    Get(Get),
    Publish(Publish),
    Set(Set),
    Subscribe(Subscribe),
    Unsubscribe(Unsubscribe),
    Ping(Ping),
    Unknown(Unknown),
}

impl Command {
    /// Parsea un comandos desde la trama que se ha recibido.
    /// 
    /// El 'Frame' debe representar un comanddo Redis que es soportado
    /// por nuestro 'mini-redis'.
    /// 
    /// # Retorno
    /// Si el resultado es satisfactorio, se retorna un 'Command' y
    /// en caso de error se retorna un 'Err'.
    pub fn from_frame(frame: Frame) -> crate::Result<Command> {
        // El valor de la trama es decorado con un `Parse`el cual
        // proporciona una API tipo "cursor" el cual permite un 
        // parseado mas sencillo.
        //
        // El valor de la trame debe ser una variante de un erray. Cualquier 
        // otra variante resultara en el retorno de un error.
        let mut parse = Parse::new(frame)?;

        // Todos los comandos redis empiezan con una string con 
        // el nombre del comando. El nombre es leido y convertido a minisculas
        // para podes establecer cual es el comando.
        let command_name = parse.next_string()?.to_lowercase();

        // Se busca la coincidencia del comando para delegar el resto del comando
        // especificamente a cada comando.
        let command = match &command_name[..] {
            "get" => Command::Get(Get::parse_frames(&mut parse)?),
            "publish" => Command::Publish(Publish::parse_frames(&mut parse)?),
            "set" => Command::Set(Set::parse_frames(&mut parse)?),
            "subscribe" => Command::Subscribe(Subscribe::parse_frames(&mut parse)?),
            "unsubscribe" => Command::Unsubscribe(Unsubscribe::parse_frames(&mut parse)?),
            "ping" => Command::Ping(Ping::parse_frames(&mut parse)?),
            _ => {
                // Si el comando no es reconocido se retornara un
                // comando Unknown ya que 
                //
                // Ademas se retorna ya sin esperar a que se ejecute
                // el parse.finish().
                return Ok(Command::Unknown(Unknown::new(command_name)));
            }
        };

        // Verifica si quedan frame spendientes de consumer despues de invocar
        // al parseado de cada comando. Si hay campos pendientes aunque el 
        // parseado de comando haya resultado satisfactorio indicara que hay 
        // mas campos de los permitidos y un error sera retornado.
        parse.finish()?;

        // El comando ha sido parseado satisfactoriamente.
        Ok(command)
    }

    /// Aplica el comando a la base de datos.
    /// 
    /// La respuesta es escrita en `dst'. 
    pub(crate) async fn apply(
        self,
        db: &Db,
        dst: &mut Connection,
        shutdown: &mut Shutdown,
    ) -> crate::Result<()> {
        use Command::*;

        match self {
            Get(cmd) => cmd.apply(db, dst).await,
            Publish(cmd) => cmd.apply(db, dst).await,
            Set(cmd) => cmd.apply(db, dst).await,
            Subscribe(cmd) => cmd.apply(db, dst, shutdown).await,
            Ping(cmd) => cmd.apply(dst).await,
            Unknown(cmd) => cmd.apply(dst).await,
            // `Unsubscribe` no puede ser aplicado, el es solo recibiso 
            // desde el contexto de un comando 'Subscribe'.
            Unsubscribe(_) => Err("`Unsubscribe` is unsupported in this context".into()),
        }
    }

    /// Retorna el nombre del comando
    pub(crate) fn get_name(&self) -> &str {
        match self {
            Command::Get(_) => "get",
            Command::Publish(_) => "pub",
            Command::Set(_) => "set",
            Command::Subscribe(_) => "subscribe",
            Command::Unsubscribe(_) => "unsubscribe",
            Command::Ping(_) => "ping",
            Command::Unknown(cmd) => cmd.get_name(),
        }
    }
}
