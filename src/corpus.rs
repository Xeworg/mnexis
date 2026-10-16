//! Deterministic corpus generator — pure index arithmetic, no RNG.
//!
//! ## Design
//!
//! 200 topic pools (`pool[i] = pools[i % 200]`) each contain Spanish and
//! English template sentences.  Document `i` composes 2–6 sentences from its
//! pool, always embeds the unique token `mnx<i>`, and records a varied
//! character length (≈ 200–2 000 chars).
//!
//! State distribution:
//! - `i % 20 == 7`  → superseded
//! - `i % 20 == 13` → deleted  (soft delete, `deleted=1`)
//! - otherwise       → active   (~5 % each bucket × 18 active / 200 = 90 %)
//!
//! Namespace distribution:
//! - `proyecto-a` … `proyecto-e` cycling by `i % 5`.
//!
//! UUID format: `00000000-0000-0000-0000-<i zero-padded to 12>`.
//!
//! ## Query sets
//!
//! **Relevant queries (200):** each query targets one ACTIVE document `i`
//! (superseded/deleted documents are excluded because the default search
//! mode filters them by design).  The query is the unique token `mnx<i>`
//! plus one topic word from the template document `i` always includes, so
//! the target matches every term while same-pool documents compete on the
//! topic word alone.  By construction the relevant set is exactly `{i}`.
//!
//! **No-answer queries (50):** pairs of words from an even-numbered pool
//! and an odd-numbered pool, VERIFIED so that no single pool's token set
//! contains both words.  Because each document's content is exactly one
//! pool's templates plus its unique `mnx<i>` token, FTS5 implicit AND
//! returns zero rows by construction.

pub use crate::retrieval::Memory;

/// Number of topic pools.
pub const NUM_POOLS: usize = 200;
/// Number of relevant queries (one per document).
pub const NUM_RELEVANT: usize = NUM_POOLS; // 200
/// Number of no-answer queries.
pub const NUM_NO_ANSWER: usize = 50;

/// A query along with the set of document IDs that are expected-relevant.
#[derive(Debug, Clone)]
pub struct Query {
    /// FTS5 query text (space-separated terms).
    pub text: String,
    /// Document IDs that are the correct answers.
    pub relevant: Vec<i64>,
}

// ── Topic pools ────────────────────────────────────────────────────────────────

/// One sentence template with its source language tag.
#[derive(Clone)]
#[allow(dead_code)]
struct Template(&'static str, &'static str); // (sentence, lang-tag)

/// 200 topic pools.  Each pool alternates Spanish / English sentences so that
/// every pool produces mixed-language documents.
fn topic_pools() -> Vec<Vec<Template>> {
    let mut pools: Vec<Vec<Template>> = Vec::new();
    pools.extend([
        // Pool 0 — Graph algorithms
        vec![
            Template(
                "El algoritmo de Dijkstra encuentra el camino más corto en grafos con pesos no negativos.",
                "es",
            ),
            Template(
                "Breadth-first search explores a graph level by level using a queue.",
                "en",
            ),
            Template(
                "Los grafos dirigidos cíclicos pueden detectarse mediante DFS con marcas de visita.",
                "es",
            ),
            Template(
                "Kruskal's algorithm builds a minimum spanning tree by sorting edges.",
                "en",
            ),
            Template(
                "La componente de conectividad de un grafo puede hallarse mediante un recorrido DFS simple.",
                "es",
            ),
            Template(
                "Topological sort works only on directed acyclic graphs.",
                "en",
            ),
            Template(
                "El algoritmo de Bellman-Ford maneja aristas con pesos negativos.",
                "es",
            ),
            Template(
                "A strongly connected component contains vertices reachable from each other.",
                "en",
            ),
        ],
        // Pool 1 — Relational databases
        vec![
            Template(
                "La normalización reduce la redundancia en las bases de datos relacionales.",
                "es",
            ),
            Template(
                "ACID properties guarantee reliable database transactions.",
                "en",
            ),
            Template(
                "El join interno entre tablas elimina los registros sin correspondencia.",
                "es",
            ),
            Template(
                "An index on a primary key speeds up lookup operations significantly.",
                "en",
            ),
            Template(
                "La clave primaria identifica cada fila de manera única en la tabla.",
                "es",
            ),
            Template(
                "A foreign key constraint enforces referential integrity between tables.",
                "en",
            ),
            Template(
                "El EXPLAIN muestra el plan de ejecución de una consulta SQL.",
                "es",
            ),
            Template(
                "Deadlocks can occur when transactions hold locks in a circular dependency.",
                "en",
            ),
        ],
        // Pool 2 — Programming languages
        vec![
            Template("Python soporta programación orientada a objetos y funcional.", "es"),
            Template("Rust guarantees memory safety without a garbage collector.", "en"),
            Template(
                "Los closures en JavaScript capturan el ámbito léxico de la función externa.",
                "es",
            ),
            Template("Go uses goroutines and channels for concurrent programming.", "en"),
            Template(
                "TypeScript añade tipado estático opcional a JavaScript.",
                "es",
            ),
            Template("Haskell uses lazy evaluation and pure functional programming.", "en"),
            Template("Ruby on Rails implementa el patrón MVC para aplicaciones web.", "es"),
            Template(
                "The JVM enables Java bytecode to run on any platform with a compatible runtime.",
                "en",
            ),
        ],
        // Pool 3 — Rust programming
        vec![
            Template(
                "El ownership system de Rust elimina los errores de uso-after-free.",
                "es",
            ),
            Template("Borrow checking prevents data races at compile time.", "en"),
            Template(
                "Las lifetimes en Rust indican cuánto tiempo una referencia es válida.",
                "es",
            ),
            Template("Cargo is the package manager and build tool for Rust projects.", "en"),
            Template(
                "El trait Debug permite formatear estructuras para depuración.",
                "es",
            ),
            Template("Async/await syntax simplifies writing concurrent Rust code.", "en"),
            Template(
                "El Option y Result types obligan a manejar los casos de error.",
                "es",
            ),
            Template(
                "Unsafe Rust bypasses the borrow checker for performance-critical sections.",
                "en",
            ),
        ],
        // Pool 4 — Neural networks
        vec![
            Template(
                "La backpropagation calcula gradientes capa por capa en redes neuronales.",
                "es",
            ),
            Template(
                "ReLU activation introduces non-linearity and helps with gradient flow.",
                "en",
            ),
            Template(
                "El gradient descent actualiza los pesos en la dirección opuesta al gradiente.",
                "es",
            ),
            Template(
                "Dropout regularisation prevents overfitting by randomly silencing neurons.",
                "en",
            ),
            Template(
                "La función de pérdida mide qué tan lejos está la predicción del valor real.",
                "es",
            ),
            Template(
                "Convolutional layers extract spatial features from images efficiently.",
                "en",
            ),
            Template(
                "El learning rate controla el tamaño de los pasos de actualización de pesos.",
                "es",
            ),
            Template(
                "Batch normalisation stabilises training by normalising layer inputs.",
                "en",
            ),
        ],
        // Pool 5 — Distributed systems
        vec![
            Template(
                "El teorema CAP indica que un sistema no puede garantizar simultáneamente consistencia, disponibilidad y tolerancia a particiones.",
                "es",
            ),
            Template(
                "Raft is a consensus algorithm designed to be understandable and implementable.",
                "en",
            ),
            Template(
                "Los mensajes heartbeat mantienen la detección de fallos entre nodos.",
                "es",
            ),
            Template(
                "Eventual consistency allows replicas to diverge temporarily before converging.",
                "en",
            ),
            Template(
                "La replicación master-slave distribuye la carga de lectura en el sistema.",
                "es",
            ),
            Template(
                "Two-phase commit ensures atomic commitment across distributed transactions.",
                "en",
            ),
            Template(
                "Los vectores de reloj capturan la causalidad entre eventos distribuidos.",
                "es",
            ),
            Template(
                "Consistent hashing minimises redistribution when nodes join or leave a cluster.",
                "en",
            ),
        ],
        // Pool 6 — Compilers
        vec![
            Template("El análisis léxico convierte el código fuente en una secuencia de tokens.", "es"),
            Template(
                "LR parsers build a parse tree from the bottom up using a stack.",
                "en",
            ),
            Template(
                "La tabla de símbolos almacena información sobre identificadores y sus tipos.",
                "es",
            ),
            Template(
                "Constant folding evaluates constant expressions during compilation.",
                "en",
            ),
            Template(
                "El intermediate representation permite optimizaciones independientes de la arquitectura.",
                "es",
            ),
            Template(
                "Register allocation maps variables to a finite set of CPU registers.",
                "en",
            ),
            Template(
                "La generación de código produce instrucciones máquina o bytecode a partir del IR.",
                "es",
            ),
            Template(
                "Dead code elimination removes unreachable or unused code from the program.",
                "en",
            ),
        ],
        // Pool 7 — Operating systems
        vec![
            Template(
                "El scheduler de shortest job next minimiza el tiempo de espera promedio.",
                "es",
            ),
            Template("Memory paging divides address space into fixed-size pages.", "en"),
            Template(
                "La memoria virtual permite que un proceso use más memoria de la físicamente disponible.",
                "es",
            ),
            Template(
                "Context switching saves and restores CPU registers to switch between processes.",
                "en",
            ),
            Template(
                "El problema de los filósofos comensales ilustra la sincronización de procesos.",
                "es",
            ),
            Template("Semaphores coordinate access to shared resources among threads.", "en"),
            Template("Los sistemas de archivos journaling registran cambios antes de aplicarlos.", "es"),
            Template(
                "Copy-on-write optimises fork() by sharing memory pages until they are modified.",
                "en",
            ),
        ],
        // Pool 8 — Networking
        vec![
            Template(
                "El three-way handshake establece una conexión TCP confiable entre cliente y servidor.",
                "es",
            ),
            Template(
                "DNS translates human-readable domain names into IP addresses.",
                "en",
            ),
            Template(
                "El protocolo HTTP sin estado permite arquitecturas REST stateless.",
                "es",
            ),
            Template("TLS provides encryption and authentication for network communications.", "en"),
            Template(
                "La NAT traduce direcciones IP privadas a públicas para comunicación externa.",
                "es",
            ),
            Template(
                "BGP is the routing protocol used between autonomous systems on the internet.",
                "en",
            ),
            Template("El protocolo UDP envía datagramas sin garantía de entrega ordenada.", "es"),
            Template(
                "ARP resolves IP addresses to MAC addresses on a local network.",
                "en",
            ),
        ],
        // Pool 9 — Data structures
        vec![
            Template(
                "Una tabla hash proporciona acceso promedio O(1) mediante funciones de dispersión.",
                "es",
            ),
            Template(
                "A red-black tree self-balances to maintain O(log n) operations.",
                "en",
            ),
            Template(
                "Las colas FIFO garantizan que los elementos se procesen en orden de llegada.",
                "es",
            ),
            Template("Binary heaps efficiently support priority queue operations.", "en"),
            Template(
                "Los árboles B optimizan operaciones en sistemas con almacenamiento secundario.",
                "es",
            ),
            Template(
                "Trie data structures enable fast prefix-based string lookups.",
                "en",
            ),
            Template(
                "Las listas enlazadas permiten inserciones O(1) pero búsqueda O(n).",
                "es",
            ),
            Template(
                "Disjoint set unions track partition of elements into non-overlapping subsets.",
                "en",
            ),
        ],
        // Pool 10 — Concurrency
        vec![
            Template(
                "Los mutex bloquean el acceso exclusivo a recursos compartidos entre hilos.",
                "es",
            ),
            Template(
                "Condition variables let threads wait for arbitrary conditions efficiently.",
                "en",
            ),
            Template(
                "Las promises en JavaScript representan valores futuros de operaciones asíncronas.",
                "es",
            ),
            Template("Futures in Scala encapsulate delayed computations and their results.", "en"),
            Template(
                "El modelo de actores procesa mensajes de forma concurrente sin memoria compartida.",
                "es",
            ),
            Template(
                "Software transactional memory uses transactions to coordinate concurrent memory access.",
                "en",
            ),
            Template(
                "Los spinlocks esperan activamente mediante busy waiting.",
                "es",
            ),
            Template(
                "Read-write locks allow multiple readers or a single writer at any time.",
                "en",
            ),
        ],
        // Pool 11 — Cloud computing
        vec![
            Template("AWS EC2 proporciona instancias virtuales escalables bajo demanda.", "es"),
            Template(
                "Containers share the host OS kernel and start in milliseconds.",
                "en",
            ),
            Template(
                "Kubernetes orquestra contenedores en clústeres de producción.",
                "es",
            ),
            Template(
                "Serverless functions abstract away server management entirely.",
                "en",
            ),
            Template("El autoescalado ajusta la capacidad según métricas de uso.", "es"),
            Template("CDN edge nodes cache content close to end users globally.", "en"),
            Template(
                "Terraform gestiona infraestructura como código en múltiples proveedores.",
                "es",
            ),
            Template(
                "A service mesh provides observability and traffic management for microservices.",
                "en",
            ),
        ],
        // Pool 12 — Security
        vec![
            Template("La autenticación de dos factores añade una capa extra de seguridad.", "es"),
            Template(
                "Public key cryptography enables secure communication without a shared secret.",
                "en",
            ),
            Template(
                "El hashing de contraseñas con sal protege contra ataques de tabla arcoíris.",
                "es",
            ),
            Template(
                "SQL injection exploits unsanitised user input in database queries.",
                "en",
            ),
            Template("Los certificados X.509 firman digitalmente comunicaciones TLS.", "es"),
            Template("CSRF tokens prevent cross-site request forgery attacks.", "en"),
            Template(
                "El principio de mínimo privilegio limita el acceso al mínimo necesario.",
                "es",
            ),
            Template(
                "Memory-safe languages prevent entire classes of buffer overflow vulnerabilities.",
                "en",
            ),
        ],
        // Pool 13 — Machine learning
        vec![
            Template(
                "El sobreajuste ocurre cuando el modelo memoriza datos de entrenamiento en lugar de generalizar.",
                "es",
            ),
            Template(
                "Cross-validation assesses model generalisation by testing on unseen data splits.",
                "en",
            ),
            Template(
                "La validación cruzada evalúa la generalización del modelo en datos no vistos.",
                "es",
            ),
            Template(
                "Hyperparameter tuning explores the best configuration for a learning algorithm.",
                "en",
            ),
            Template(
                "El sesgo-varianza trade-off equilibra subestimación y sobreestimación.",
                "es",
            ),
            Template(
                "Ensemble methods combine multiple models to improve predictive performance.",
                "en",
            ),
            Template("La regularización L1 produce pesos dispersos en modelos lineales.", "es"),
            Template(
                "Grid search systematically evaluates all combinations of hyperparameter values.",
                "en",
            ),
        ],
        // Pool 14 — Web development
        vec![
            Template(
                "React usa un virtual DOM para minimizar operaciones costosas del DOM real.",
                "es",
            ),
            Template(
                "REST APIs use standard HTTP methods to perform CRUD operations on resources.",
                "en",
            ),
            Template(
                "GraphQL permite a los clientes solicitar exactamente los datos que necesitan.",
                "es",
            ),
            Template(
                "JWT tokens authenticate users without server-side session storage.",
                "en",
            ),
            Template(
                "El patrón observer notifica a los componentes cuando cambia el estado.",
                "es",
            ),
            Template(
                "Webpack bundles JavaScript modules and their dependencies into a single file.",
                "en",
            ),
            Template(
                "El protocolo WebSocket establece comunicación bidireccional en tiempo real.",
                "es",
            ),
            Template(
                "CSS Grid enables two-dimensional layout with rows and columns simultaneously.",
                "en",
            ),
        ],
        // Pool 15 — Testing
        vec![
            Template(
                "Las pruebas unitarias verifican el comportamiento de unidades individuales de código.",
                "es",
            ),
            Template(
                "Property-based testing generates random inputs to verify invariant properties.",
                "en",
            ),
            Template(
                "El mocking sustituye dependencias reales por implementaciones controladas en pruebas.",
                "es",
            ),
            Template(
                "Code coverage measures how many lines of code are exercised by tests.",
                "en",
            ),
            Template(
                "La integración continua ejecuta pruebas automáticamente tras cada commit.",
                "es",
            ),
            Template(
                "Mutation testing evaluates the quality of a test suite by inserting bugs.",
                "en",
            ),
            Template(
                "Las pruebas de regresión aseguran que cambios nuevos no rompan funcionalidad existente.",
                "es",
            ),
            Template(
                "Fuzz testing feeds random data to a program to uncover crashes and panics.",
                "en",
            ),
        ],
        // Pool 16 — APIs
        vec![
            Template(
                "El versionado semántico indica compatibilidades mayor, menor y de parche.",
                "es",
            ),
            Template(
                "Rate limiting protects APIs from abuse and ensures fair resource usage.",
                "en",
            ),
            Template(
                "Los webhooks notifican a clientes sobre eventos mediante HTTP callbacks.",
                "es",
            ),
            Template(
                "OpenAPI specifications document REST APIs in a machine-readable format.",
                "en",
            ),
            Template(
                "La paginación evita devolver resultados excesivos mediante límites y desplazamiento.",
                "es",
            ),
            Template("OAuth 2.0 delegates authorisation without sharing user credentials.", "en"),
            Template("Los errores 429 indican que el cliente ha superado el límite de tasa.", "es"),
            Template(
                "Idempotent operations produce the same result regardless of how many times they run.",
                "en",
            ),
        ],
        // Pool 17 — Data engineering
        vec![
            Template("Apache Kafka topics store ordered, immutable sequences of records.", "es"),
            Template(
                "ETL pipelines extract, transform and load data from sources into a data warehouse.",
                "en",
            ),
            Template(
                "La desnormalización mejora el rendimiento de lectura al duplicar datos.",
                "es",
            ),
            Template(
                "Data lake architecture stores raw data in its native format at scale.",
                "en",
            ),
            Template(
                "Los schema registries validan la compatibilidad de formatos de mensajes.",
                "es",
            ),
            Template(
                "Airflow orchestrates complex workflows defined as directed acyclic graphs.",
                "en",
            ),
            Template("La deduplicación elimina registros duplicados basándose en claves únicas.", "es"),
            Template(
                "Columnar storage formats like Parquet optimise analytical query performance.",
                "en",
            ),
        ],
        // Pool 18 — DevOps
        vec![
            Template("Docker compose define y ejecuta aplicaciones multi-contenedor.", "es"),
            Template(
                "Infrastructure as code treats server configuration like software source code.",
                "en",
            ),
            Template(
                "El blue-green deployment reduce riesgos al mantener dos entornos idénticos.",
                "es",
            ),
            Template(
                "Canary releases gradually roll out changes to a small subset of users.",
                "en",
            ),
            Template(
                "La monitorización con Prometheus recoge métricas time-series de servicios.",
                "es",
            ),
            Template(
                "Grafana dashboards visualise system metrics and application performance.",
                "en",
            ),
            Template(
                "Los runbooks documentan procedimientos operativos para responder a incidentes.",
                "es",
            ),
            Template("SLOs define the reliability targets that the system must meet.", "en"),
        ],
        // Pool 19 — Version control
        vec![
            Template(
                "Git rebase reescribe el historial de commits aplicando cambios sobre otra rama.",
                "es",
            ),
            Template(
                "Merge conflicts arise when changes in different branches affect the same lines.",
                "en",
            ),
            Template(
                "El squash fusiona múltiples commits en uno solo antes de fusionar ramas.",
                "es",
            ),
            Template(
                "Branching strategies like GitFlow structure release and development workflows.",
                "en",
            ),
            Template(
                "Los hooks pre-commit validan código antes de registrar un commit.",
                "es",
            ),
            Template(
                "Git bisect uses binary search to find the commit that introduced a bug.",
                "en",
            ),
            Template("La rama main contiene la historia canonical del proyecto.", "es"),
            Template(
                "Cherry-picking applies specific commits from one branch to another.",
                "en",
            ),
        ],
        // Pool 20 — Caching
        vec![
            Template(
                "Redis almacena datos en memoria para acceso de ultra baja latencia.",
                "es",
            ),
            Template(
                "Cache invalidation strategies determine when stored data becomes stale.",
                "en",
            ),
            Template(
                "Los patrones cache-aside reducen la carga de la base de datos principal.",
                "es",
            ),
            Template(
                "LRU eviction discards the least recently used item when the cache is full.",
                "en",
            ),
            Template(
                "El tiempo-to-live indica cuánto tiempo un dato permanece válido en caché.",
                "es",
            ),
            Template(
                "Write-through caching writes to both cache and storage simultaneously.",
                "en",
            ),
            Template(
                "La caché de CPU almacena datos de uso frecuente para acelerar el acceso.",
                "es",
            ),
            Template(
                "Distributed caches replicate data across multiple nodes for scalability.",
                "en",
            ),
        ],
        // Pool 21 — Performance
        vec![
            Template(
                "El profiling identifica cuellos de botella midiendo tiempo de ejecución por función.",
                "es",
            ),
            Template(
                "Amdahl's law quantifies the maximum speedup from parallelisation.",
                "en",
            ),
            Template(
                "La latencia p99 indica el tiempo por debajo del cual caen el 99% de las operaciones.",
                "es",
            ),
            Template("JIT compilation optimises hot code paths at runtime.", "en"),
            Template(
                "El batching combina múltiples operaciones pequeñas para reducir overhead.",
                "es",
            ),
            Template(
                "Connection pooling reuses established database connections to avoid setup cost.",
                "en",
            ),
            Template(
                "Los índices covering permiten responder consultas sin acceder a la tabla principal.",
                "es",
            ),
            Template("Vertical scaling adds more resources to a single machine.", "en"),
        ],
        // Pool 22 — Configuration management
        vec![
            Template(
                "Environment variables isolate configuration across deployment stages.",
                "es",
            ),
            Template(
                "Feature flags enable gradual rollouts and A/B testing without redeployment.",
                "en",
            ),
            Template(
                "Los secretos deben almacenarse en bóvedas cifradas, nunca en repositorios.",
                "es",
            ),
            Template(
                "Configuration drift occurs when deployed systems diverge from the desired state.",
                "en",
            ),
            Template(
                "El twelve-factor app recomienda almacenar configuración en el entorno.",
                "es",
            ),
            Template(
                "Hot reloading applies configuration changes without restarting the service.",
                "en",
            ),
            Template(
                "Los perfiles de Spring permiten definir comportamientos específicos por entorno.",
                "es",
            ),
            Template(
                "Dynamic configuration systems update behaviour at runtime without redeployment.",
                "en",
            ),
        ],
        // Pool 23 — Observability
        vec![
            Template(
                "OpenTelemetry proporciona instrumentación estándar para trazas, métricas y logs.",
                "es",
            ),
            Template(
                "Distributed tracing follows a request across service boundaries.",
                "en",
            ),
            Template(
                "Los dashboards de Grafana muestran métricas clave del sistema en tiempo real.",
                "es",
            ),
            Template(
                "Alert fatigue occurs when too many alerts reduce team responsiveness.",
                "en",
            ),
            Template(
                "El SLO compliance se mide como el porcentaje de tiempo dentro de los objetivos.",
                "es",
            ),
            Template(
                "Structured logging uses consistent JSON fields for searchable log entries.",
                "en",
            ),
            Template(
                "La correlación de logs conecta eventos entre servicios mediante identificadores.",
                "es",
            ),
            Template(
                "USE method guides resource utilisation, saturation, and error analysis.",
                "en",
            ),
        ],
        // Pool 24 — Agile practices
        vec![
            Template(
                "El sprint review demuestra el incremento listo para las partes interesadas.",
                "es",
            ),
            Template(
                "User stories capture requirements from the perspective of the end user.",
                "en",
            ),
            Template(
                "Los equipos cross-funcionales poseen todas las habilidades para entregar valor.",
                "es",
            ),
            Template(
                "Retrospectives identify and implement improvements for the next iteration.",
                "en",
            ),
            Template(
                "El definition of done establece criterios claros para considerar una tarea completa.",
                "es",
            ),
            Template(
                "Continuous improvement encourages small, incremental changes over time.",
                "en",
            ),
            Template(
                "Los backlogs priorizados organizan el trabajo por valor y riesgo.",
                "es",
            ),
            Template(
                "Burndown charts track remaining work against time in an iteration.",
                "en",
            ),
        ],
    ]);
    // Pools 25–199: generated cyclically from the 25 base pools.
    // This gives exactly 200 distinct pool indices while keeping the
    // explicit template list manageable.
    let base_count = pools.len(); // 25
    for i in base_count..NUM_POOLS {
        pools.push(pools[i % base_count].clone());
    }
    pools
}

/// Returns the pool index for document `i` (pure index arithmetic, no RNG).
fn pool_index(i: usize) -> usize {
    i % NUM_POOLS
}

/// Returns the namespace for document `i`.
fn namespace_for(i: usize) -> String {
    let ns = match i % 5 {
        0 => "proyecto-a",
        1 => "proyecto-b",
        2 => "proyecto-c",
        3 => "proyecto-d",
        _ => "proyecto-e",
    };
    ns.to_owned()
}

/// Returns the state and deleted flag for document `i`.
fn state_for(i: usize) -> (String, i32) {
    if i % 20 == 7 {
        ("superseded".to_owned(), 0)
    } else if i % 20 == 13 {
        ("active".to_owned(), 1) // soft-deleted
    } else {
        ("active".to_owned(), 0)
    }
}

/// Returns the UUID for document `i`: `00000000-0000-0000-0000-<i padded to 12>`.
fn uuid_for(i: usize) -> String {
    format!("00000000-0000-0000-0000-{:012}", i)
}

/// Returns the number of sentences for document `i` based on its index.
fn num_sentences(i: usize) -> usize {
    // 2–6 sentences, varied by index arithmetic
    let base = (i % 5) + 2; // 2..=6
    base
}

/// Returns the `topic_key` for document `i`.
fn topic_key_for(i: usize) -> String {
    format!("topic.{:03}", i % NUM_POOLS)
}

// ── Corpus generation ─────────────────────────────────────────────────────────

/// Generate document `i` as a `Memory`.
pub fn generate_document(i: usize) -> Memory {
    let pools = topic_pools();
    let pool_idx = pool_index(i);
    let pool: &[Template] = &pools[pool_idx]; // pool_idx in 0..200

    let ns = namespace_for(i);
    let (state, deleted) = state_for(i);
    let num_sents = num_sentences(i);

    // Build content: 2–6 sentences from the pool
    let mut sentences = Vec::with_capacity(num_sents);
    for j in 0..num_sents {
        let t = &pool[(i + j) % pool.len()];
        sentences.push(t.0.to_owned());
    }

    // Compose the content for FTS5: just the sentences and the mnx token.
    // The UUID is stored as a separate column and not part of the FTS content,
    // preventing FTS5 from splitting the hyphenated UUID and corrupting tokens.
    let content = format!("{} - mnx{}", sentences.join(" "), i,);

    // Rough character length check (just for documentation / summary stats)
    let _char_count = content.chars().count();

    Memory {
        id: None,
        uuid: uuid_for(i),
        namespace: ns,
        topic_key: topic_key_for(i),
        memory_type: "fact".to_owned(),
        content,
        state,
        revision: 1,
        deleted,
        created_at: "2025-01-01T00:00:00Z".to_owned(),
        valid_from: None,
        valid_until: None,
        assertion_basis: Some("corpus-generator".to_owned()),
    }
}

/// Generate all `n` documents.
pub fn generate_corpus(n: usize) -> Vec<Memory> {
    (0..n).map(generate_document).collect()
}

/// Extract a query-safe word from pool `pool_idx` for query construction.
///
/// Walks the template selected by `offset % pool.len()` backwards from its
/// last word and returns the first token that, after stripping trailing
/// punctuation, is a plain alphanumeric FTS5 bareword. Hyphenated tokens
/// such as "uso-after-free" or "Bellman-Ford" are NOT valid single-term
/// FTS5 query syntax (the parser reads the inner segment as a column
/// reference and fails with "no such column"), so they must never become
/// query terms. Because the word always comes from the same template the
/// target document includes, the document still matches the full AND query.
fn topic_word_for_pool(pool_idx: usize, offset: usize) -> &'static str {
    let pools = topic_pools();
    let pool: &[Template] = &pools[pool_idx]; // pool_idx in 0..200
    let t = &pool[offset % pool.len()];
    let words: Vec<&str> = t.0.split_whitespace().collect();
    words
        .iter()
        .rev()
        .map(|w| w.trim_end_matches(&['.', ',', ';', ':', '!', '?'][..]))
        .find(|w| !w.is_empty() && w.chars().all(|c| c.is_alphanumeric()))
        .unwrap_or(&"topic")
}

/// Build the 200 relevant queries, each targeting one ACTIVE document.
///
/// The query is the document's unique token `mnx<i>` plus one topic word
/// taken from the template that document always includes (its first
/// sentence, template `i % pool.len()`), so the target matches every query
/// term while same-pool documents compete on the topic word alone.
/// Targets are restricted to active, non-deleted documents because the
/// default search mode excludes superseded and deleted rows by design;
/// measuring those as misses would conflate corpus construction with
/// retrieval quality.
pub fn relevant_queries() -> Vec<Query> {
    let mut queries = Vec::with_capacity(NUM_RELEVANT);
    let mut i = 0usize;
    while queries.len() < NUM_RELEVANT {
        let (state, deleted) = state_for(i);
        if state == "active" && deleted == 0 {
            let word = topic_word_for_pool(pool_index(i), i);
            queries.push(Query {
                text: format!("mnx{} {}", i, word),
                relevant: vec![(i + 1) as i64],
            });
        }
        i += 1;
    }
    queries
}

/// Lowercased alphanumeric token set of one pool's template union — the same
/// tokenization FTS5's unicode61 tokenizer applies to document content.
fn pool_token_set(pool: &[Template]) -> std::collections::HashSet<String> {
    pool.iter()
        .flat_map(|t| t.0.split(|c: char| !c.is_alphanumeric()))
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
        .collect()
}

/// True when some pool's token set contains both words, i.e. a document
/// built from that pool would match the pair.
fn pair_cooccurs(token_sets: &[std::collections::HashSet<String>], a: &str, b: &str) -> bool {
    let a = a.to_lowercase();
    let b = b.to_lowercase();
    token_sets
        .iter()
        .any(|set| set.contains(&a) && set.contains(&b))
}

/// Build the 50 no-answer queries.
///
/// Strategy: pair words from an even-numbered pool with words from an
/// odd-numbered pool, skipping any pair whose two words co-occur inside a
/// single pool's token set. Because each document's content is exactly one
/// pool's templates plus its unique `mnx<i>` token, a verified pair can
/// never match a document, so FTS5 implicit AND returns zero rows by
/// construction.
pub fn no_answer_queries() -> Vec<Query> {
    let pools = topic_pools();
    let token_sets: Vec<std::collections::HashSet<String>> =
        pools.iter().map(|p| pool_token_set(p)).collect();

    let even_pools: Vec<usize> = (0..NUM_POOLS).step_by(2).take(10).collect();
    let odd_pools: Vec<usize> = (1..NUM_POOLS).step_by(2).take(10).collect();

    let mut pairs: Vec<Query> = Vec::with_capacity(NUM_NO_ANSWER);
    'outer: for ep in &even_pools {
        for op in &odd_pools {
            // Vary which template within each pool contributes the word.
            for wk in 0..4 {
                let (te, to) = match wk {
                    0 => (0, 1),
                    1 => (1, 0),
                    2 => (2, 2),
                    _ => (3, 3),
                };
                let word_e = topic_word_for_pool(*ep, te);
                let word_o = topic_word_for_pool(*op, to);
                if word_e == word_o || pair_cooccurs(&token_sets, word_e, word_o) {
                    continue;
                }
                pairs.push(Query {
                    text: format!("{} {}", word_e, word_o),
                    relevant: vec![],
                });
                if pairs.len() >= NUM_NO_ANSWER {
                    break 'outer;
                }
            }
        }
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_documents_contain_unique_token() {
        for i in 0..1000 {
            let doc = generate_document(i);
            let token = format!("mnx{}", i);
            assert!(
                doc.content.contains(&token),
                "doc {} missing token mnx{}",
                i,
                i
            );
        }
    }

    #[test]
    fn superseded_and_deleted_distribution() {
        let mut superseded = 0usize;
        let mut deleted = 0usize;
        let mut active = 0usize;
        for i in 0..1000 {
            let (state, del) = state_for(i);
            if state == "superseded" {
                superseded += 1;
            } else if del == 1 {
                deleted += 1;
            } else {
                active += 1;
            }
        }
        // 20-cycle: 1 superseded, 1 deleted per 20 → 5% each
        assert_eq!(superseded, 1000 / 20, "5% superseded expected");
        assert_eq!(deleted, 1000 / 20, "5% deleted expected");
        assert_eq!(
            active,
            1000 - 100,
            "90% active expected (1000 - 50 superseded - 50 deleted)"
        );
    }

    #[test]
    fn relevant_queries_target_exactly_one_document() {
        let queries = relevant_queries();
        assert_eq!(queries.len(), NUM_RELEVANT);
        for q in &queries {
            assert_eq!(
                q.relevant.len(),
                1,
                "each relevant query should have exactly one target"
            );
        }
    }

    #[test]
    fn no_answer_queries_have_empty_relevant_set() {
        let queries = no_answer_queries();
        assert_eq!(queries.len(), NUM_NO_ANSWER);
        for q in &queries {
            assert!(
                q.relevant.is_empty(),
                "no-answer query must have empty relevant set"
            );
        }
    }

    #[test]
    fn relevant_queries_target_only_active_documents() {
        for q in relevant_queries() {
            for &rowid in &q.relevant {
                let index = (rowid - 1) as usize;
                let doc = generate_document(index);
                assert_eq!(
                    doc.state, "active",
                    "relevant query target {index} must be an active document"
                );
                assert_eq!(
                    doc.deleted, 0,
                    "relevant query target {index} must not be soft-deleted"
                );
            }
        }
    }

    #[test]
    fn no_answer_query_terms_never_cooccur_within_one_pool() {
        let pools = topic_pools();
        for q in no_answer_queries() {
            let terms: Vec<&str> = q.text.split_whitespace().collect();
            assert_eq!(terms.len(), 2, "no-answer query must have two terms");
            for pool in &pools {
                let tokens: std::collections::HashSet<String> = pool
                    .iter()
                    .flat_map(|t| t.0.split(|c: char| !c.is_alphanumeric()))
                    .filter(|t| !t.is_empty())
                    .map(|t| t.to_lowercase())
                    .collect();
                let a = terms[0].to_lowercase();
                let b = terms[1].to_lowercase();
                assert!(
                    !(tokens.contains(&a) && tokens.contains(&b)),
                    "no-answer pair '{}' / '{}' co-occurs in a pool",
                    terms[0],
                    terms[1]
                );
            }
        }
    }

    #[test]
    fn query_terms_are_clean_fts5_barewords() {
        let mut all = relevant_queries();
        all.extend(no_answer_queries());
        for q in &all {
            for term in q.text.split_whitespace() {
                assert!(
                    !term.is_empty() && term.chars().all(|c| c.is_alphanumeric()),
                    "query term {term:?} in {:?} must be a plain alphanumeric FTS5 bareword \
                     (hyphenated tokens are not valid single-term FTS5 query syntax)",
                    q.text
                );
            }
        }
    }
}
