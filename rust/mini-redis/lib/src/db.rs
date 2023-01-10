use tokio::sync::{broadcast, Notify};
use tokio::time::{self, Duration, Instant};

use bytes::Bytes;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use tracing::debug;

/// Un envoltorio alrededor de una instancia `Db`. 
/// Su funcion es permitir la limpieza ordenada de `Db` al marcar que 
/// la tarea de purga en segundo plano se cierre cuando se elimine esta estructura.
#[derive(Debug)]
pub struct DbDropGuard {
    /// La instancia de `Db` que sera desmontada cuando esta estructura 
    /// `DbDropGuard` sea eliminada (dropped).
    db: Db,
}

/// Estado del servidor comportido con todas las conexiones.
/// 
/// 'Db' contiene en su interior las estructuras de datos que almacenando
/// los key/value y tambien todos los valores `broadcast::Sender`
/// para los canales activos de pub/sub.
/// 
/// En primera instancia contiene un Arc 'Atomically Reference Counted' para 
/// poder compartir con el resto de threads estos datos.
/// 
/// Cuando un 'Db' es creado la lanza tambien una tarea. Esta tarea es 
/// utilizada para gestionar la expiracion de los valores. La tarea funcionara 
/// hasta que todas las instancias de 'Db' son borradas, momento en el que
/// terminara.
#[derive(Debug, Clone)]
pub struct Db {
    /// Gestiona el estado compartido. La tarea secundaria que gestiona 
    /// las expiraciones tambien poseera un `Arc<Shared>`.
    shared: Arc<Shared>,
}

#[derive(Debug)]
struct Shared {
    /// El estado compartido es custodiado por un mutex. Este es un `std::sync::Mutex`
    /// standar y no se utiliza la version del mutex de Tokio.
    /// Esto es asi porque no se estan realizando operaciones asincronas mientras 
    /// se mantiene ocupado el mutex. Ademas la seccion critica es muy pequeña.
    /// 
    /// Un Tokio mutex está diseñado principalmente para usarse cuando los bloqueos 
    /// deben mantenerse en los puntos de cesion `.await`. Por lo general, todos 
    /// los demás casos se atienden mejor con un mutex estándar.
    /// 
    /// Si la sección crítica no incluye ninguna operación asíncrona pero es larga 
    /// (uso intensivo de la CPU o realiza operaciones de bloqueo), entonces toda 
    /// la operación, incluida la espera del mutex, se considera una operación 
    /// de "bloqueo" y `tokio::task::spawn_blocking` debería ser usado.
    /// 
    state: Mutex<State>,

    /// Notifica el vencimiento de la entrada de manejo de tareas en segundo plano.
    /// La tarea en segundo plano espera a que se notifique esto, luego verifica 
    /// los valores caducados o la señal de parada.
    background_task: Notify,
}

#[derive(Debug)]
struct State {
    // Key/Value: Utilizamos un `std::collections::HashMap`.
    entries: HashMap<String, Entry>,

    /// Se utiliza un espacio separado para el key/value y el pub/sub. Tambien se
    /// utiliza un `std::collections::HashMap`.
    pub_sub: HashMap<String, broadcast::Sender<Bytes>>,

    /// Seguimiento de las claves TTLs
    /// 
    /// Un 'BTreeMap' se utiliza para mantener los vencimientos ordenados por 
    /// fecha de vencimiento. Esto permite a la tarea secundaria iterar por 
    /// este mapa para encontrar el siguiente valor que expira.
    /// 
    /// Aunque es poco probable, es posible que se crre un venciamiento para
    /// el mismo instante. Por ese motivo, un 'Instant' es insuficiente como clave.
    /// Un identificador unico 'u64' se utiliza para garantiza que la clave sea unica.
    expirations: BTreeMap<(Instant, u64), String>,

    /// Identificador que se utilizara para la clave compuesta de la proxima expiracion.
    next_id: u64,

    /// 'True' si la istancia de la base de datos se esta deteniendo. Esto 
    /// ocurre cuando todos los values de 'Db' han sido Drop. Asignando este
    /// valor a 'true' se marca a la tarea secundaria para que se detenga.
    shutdown: bool,
}

/// Entrada en el almacen Key/Value
#[derive(Debug)]
struct Entry {
    /// Identificador unico de la entrada.
    id: u64,

    /// Datos almazanados
    data: Bytes,

    /// Instante en el que la entrada expira y debe ser eliminada de la base de datos
    expires_at: Option<Instant>,
}

impl DbDropGuard {
    /// Crea un nuevo 'DbDropGuard' que recubre a una instancia de 'Db'.
    /// Este envoltorio permite realiza la purga de la Bd cuando esta instancia
    /// es 'droped'.
    pub(crate) fn new() -> DbDropGuard {
        DbDropGuard { 
            db: Db::new() 
        }
    }

    /// Obtiene el recurso compartido. Internamente es un 
    /// 'Arc', asi que se incremete el contador de referencias.
    pub(crate) fn db(&self) -> Db {
        self.db.clone()
    }
}

impl Drop for DbDropGuard {
    fn drop(&mut self) {
        // Marca la instancia de 'Db' para que se detenga la tarea que purga las 
        // claves que han expirado.
        self.db.shutdown_purge_task();
    }
}

impl Db {
    /// Crea una nueva instancia de 'Db' que no contiene ninguna entrada. Tambien
    /// crea la tarea que gestiona las expiraciones proporcionandole el primero
    /// clon de la base de datos.
    pub(crate) fn new() -> Db {

        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                entries: HashMap::new(),
                pub_sub: HashMap::new(),
                expirations: BTreeMap::new(),
                next_id: 0,
                shutdown: false,
            }),
            background_task: Notify::new(),
        });

        // Inicial la tarea.
        tokio::spawn(purge_expired_tasks(shared.clone()));

        // Se instancia un 'Db'
        Db { 
            shared 
        }

    }

    /// Obtiene el valor asociado con una clave.
    /// 
    /// Retorna 'None' si no hay un valor asociado con la clave. 
    /// Get the value associated with a key. Esto puede a que nunca de
    /// le asigno un valor a la clave o a que el valor expiro.
    pub(crate) fn get(&self, key: &str) -> Option<Bytes> {
        // Se adquire el bloqueo
        let state = self.shared.state.lock().unwrap();

        // Se lee la entrada y clona el valor.
        //
        // Como los datos estan almacenados utilizando 'Bytes', un clone 
        // en este caso es un clonado superficial (los datos no se copias).
        state.entries.get(key).map(|entry| entry.data.clone())
    }

    /// Establece un valor asociado con una clave junto con un periodo de
    /// vencimiento que es opcional.
    /// 
    /// Si ya hay un valor asociado con la clave, el nuevo valor substituira 
    /// al anterior.
    pub(crate) fn set(&self, key: String, value: Bytes, expire: Option<Duration>) {
        // Se adquire el bloqueo
        let mut state = self.shared.state.lock().unwrap();

        // El Id almacenado en el estado es el que se utilizara para esta operacion.
        let id = state.next_id;

        // Se incremente el Id para proxima insercion. Gracias a la 
        // proteccion del bloqueo cada operacion 'set' tiene garantizado un Id unico.
        state.next_id += 1;

        // En caso de que se haya especificado una duracion para la expiracion 
        // del valor, se convierte este duracion en el momento exacto de 
        // la expiracion.
        //
        // Tambien se programa la expiracion en el mapa de expiraciones.
        //
        // En caso de que la nueva expiracion resulta ser la proxima a ejecutar
        // se le enviara una notificacion a la tarea subyacente. 
        let (notify, expires_at) = if expire.is_some() {
            // Se calcula cuando la clave expirara.
            let when = Instant::now() + expire.unwrap();

            // Unicamente se notificara a la tarea de gestion de las expiraciones si
            // la expiracion del nuevo valor que se esta estableciendo resulta
            // ser la proxima expiracion a ejecutarse.
            let notify = state
                .next_expiration()
                .map(|expiration| expiration > when)
                .unwrap();

            // Track the expiration.
            state.expirations.insert((when, id), key.clone());

            // Resultado
            (notify, Option::Some(when))

        } else {
            (false, Option::None)
        };

        // Se asigna la clave el nuevo valor en el HashMap principal.
        // Si para esta misma clave habia un valor anterior, este se
        // obtendra como resultado de la ejecucion.
        let prev = state.entries.insert(
            key,
            Entry {
                id,
                data: value,
                expires_at,
            },
        );

        // Si previamente habia un valor asociado a la clave y ese valor tenia
        // definida una expiracion entonces hay que aliminar la correpondiente
        // entrada de mapa de expiraciones.
        if let Some(prev) = prev {
            if let Some(when) = prev.expires_at {
                // clear expiration
                state.expirations.remove(&(when, prev.id));
            }
        }

        // Se liberta el mutex antes de notificar la tarea en segundo plano. 
        // Esto ayuda a reducir la contención al evitar que la tarea en segundo 
        // plano se active y no pueda adquirir el mutex debido a que esta función 
        // aún lo retiene.
        drop(state);

        if notify {
            // Finalmente, solo se notifica a la tarea en segundo plano si necesita 
            // actualizar su estado para reflejar un nuevo vencimiento.
            self.shared.background_task.notify_one();
        }

    }

    /// Returns a `Receiver` for the requested channel.
    ///
    /// The returned `Receiver` is used to receive values broadcast by `PUBLISH`
    /// commands.
    pub(crate) fn subscribe(&self, key: String) -> broadcast::Receiver<Bytes> {
        use std::collections::hash_map::Entry;

        // Acquire the mutex
        let mut state = self.shared.state.lock().unwrap();

        // If there is no entry for the requested channel, then create a new
        // broadcast channel and associate it with the key. If one already
        // exists, return an associated receiver.
        match state.pub_sub.entry(key) {
            Entry::Occupied(e) => e.get().subscribe(),
            Entry::Vacant(e) => {
                // No broadcast channel exists yet, so create one.
                //
                // The channel is created with a capacity of `1024` messages. A
                // message is stored in the channel until **all** subscribers
                // have seen it. This means that a slow subscriber could result
                // in messages being held indefinitely.
                //
                // When the channel's capacity fills up, publishing will result
                // in old messages being dropped. This prevents slow consumers
                // from blocking the entire system.
                let (tx, rx) = broadcast::channel(1024);
                e.insert(tx);
                rx
            }
        }
    }

    /// Publish a message to the channel. Returns the number of subscribers
    /// listening on the channel.
    pub(crate) fn publish(&self, key: &str, value: Bytes) -> usize {
        let state = self.shared.state.lock().unwrap();

        state
            .pub_sub
            .get(key)
            // On a successful message send on the broadcast channel, the number
            // of subscribers is returned. An error indicates there are no
            // receivers, in which case, `0` should be returned.
            .map(|tx| tx.send(value).unwrap_or(0))
            // If there is no entry for the channel key, then there are no
            // subscribers. In this case, return `0`.
            .unwrap_or(0)
    }

    /// Signals the purge background task to shut down. This is called by the
    /// `DbShutdown`s `Drop` implementation.
    fn shutdown_purge_task(&self) {
        // The background task must be signaled to shut down. This is done by
        // setting `State::shutdown` to `true` and signalling the task.
        let mut state = self.shared.state.lock().unwrap();
        state.shutdown = true;

        // Drop the lock before signalling the background task. This helps
        // reduce lock contention by ensuring the background task doesn't
        // wake up only to be unable to acquire the mutex.
        drop(state);
        self.shared.background_task.notify_one();
    }
}

impl Shared {
    /// Purge all expired keys and return the `Instant` at which the **next**
    /// key will expire. The background task will sleep until this instant.
    fn purge_expired_keys(&self) -> Option<Instant> {
        let mut state = self.state.lock().unwrap();

        if state.shutdown {
            // The database is shutting down. All handles to the shared state
            // have dropped. The background task should exit.
            return None;
        }

        // This is needed to make the borrow checker happy. In short, `lock()`
        // returns a `MutexGuard` and not a `&mut State`. The borrow checker is
        // not able to see "through" the mutex guard and determine that it is
        // safe to access both `state.expirations` and `state.entries` mutably,
        // so we get a "real" mutable reference to `State` outside of the loop.
        let state = &mut *state;

        // Find all keys scheduled to expire **before** now.
        let now = Instant::now();

        while let Some((&(when, id), key)) = state.expirations.iter().next() {
            if when > now {
                // Done purging, `when` is the instant at which the next key
                // expires. The worker task will wait until this instant.
                return Some(when);
            }

            // The key expired, remove it
            state.entries.remove(key);
            state.expirations.remove(&(when, id));
        }

        None
    }

    /// Returns `true` if the database is shutting down
    ///
    /// The `shutdown` flag is set when all `Db` values have dropped, indicating
    /// that the shared state can no longer be accessed.
    fn is_shutdown(&self) -> bool {
        self.state.lock().unwrap().shutdown
    }
}

impl State {
    /// Desde el mapa 'expiratons' (de tipo BTreeMap<(Instant, u64), String>) se
    /// obtiene un iterador que estara ordenado de la clave.
    /// Se hace avanzar el iterador a la primera posicion para obtener la primera clave
    /// (que sera la clave con el instante mas bajo).
    /// De esta clave que esta formada por una tupla extrae el primer campo que es 
    /// el Instant.
    /// En realidad retornara un Option<Instant> ya que el caso de que el iterador 
    /// de las claves este vacio la expresion funcional retornara un 'Option.None'.
    fn next_expiration(&self) -> Option<Instant> {
        self.expirations
            .keys()
            .next()
            .map(|expiration| expiration.0)
    }
}

/// Routine executed by the background task.
///
/// Wait to be notified. On notification, purge any expired keys from the shared
/// state handle. If `shutdown` is set, terminate the task.
async fn purge_expired_tasks(shared: Arc<Shared>) {
    // If the shutdown flag is set, then the task should exit.
    while !shared.is_shutdown() {
        // Purge all keys that are expired. The function returns the instant at
        // which the **next** key will expire. The worker should wait until the
        // instant has passed then purge again.
        if let Some(when) = shared.purge_expired_keys() {
            // Wait until the next key expires **or** until the background task
            // is notified. If the task is notified, then it must reload its
            // state as new keys have been set to expire early. This is done by
            // looping.
            tokio::select! {
                _ = time::sleep_until(when) => {}
                _ = shared.background_task.notified() => {}
            }
        } else {
            // There are no keys expiring in the future. Wait until the task is
            // notified.
            shared.background_task.notified().await;
        }
    }

    debug!("Purge background task shut down")
}
