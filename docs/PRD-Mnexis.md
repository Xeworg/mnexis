# PRD — Mnexis

**Versión:** 0.5 · **Estado:** especificación propuesta, pendiente de validación mediante prototipo. Decisión posterior a v0.4: búsqueda textual FTS5 sin embeddings ni base vectorial.

> Mnexis es un motor de memoria local para agentes de IA. Conserva conocimiento deliberado, vigente y trazable; recupera información pertinente para una tarea dentro de un presupuesto de contexto; permite corregirla, exportarla y eliminarla. Se distribuye como un binario Rust, sin daemon obligatorio ni cuenta en la nube.

Nombre propuesto: **Mnexis**, comando `mnexis`. El nombre evoca memoria y conexión; lema propuesto: **La memoria que tu agente necesita.** Validar nombre, marca, repositorio y licencia antes de publicar. Este documento define comportamiento deseado; las dependencias y prestaciones pendientes no se presentan como capacidades verificadas.

## 1. Problema, propuesta y resultado esperado

Los agentes pierden decisiones entre sesiones o acumulan demasiado contexto. Guardar todo el historial produce ruido y recuperar conocimiento antiguo puede inducir errores.

Mnexis debe permitir que un agente responda: «¿Qué conocimiento vigente necesito para esta tarea, de dónde viene y cuánto contexto ocupa?». Su diferenciación propuesta es la combinación de búsqueda textual, vigencia explícita, procedencia, límites de salida y operación local sencilla. La ventaja frente a otras herramientas debe demostrarse, no asumirse.

**Caso principal:** se decidió usar `petgraph`, luego se sustituyó por un motor propio. Ante una tarea de rendimiento, Mnexis recupera la decisión actual, identifica su procedencia y no introduce la anterior como recomendación vigente. La decisión anterior sigue disponible mediante una consulta histórica autorizada.

### Usuarios

Una persona que trabaja con uno o varios agentes de programación en la misma computadora y mantiene varios proyectos. El MVP usa un perfil local, sin colaboración multiusuario.

### Resultados buscados

- Continuidad entre sesiones sin reenviar el historial completo.
- Menos errores por conocimiento sustituido o fuera del proyecto.
- Recuperación útil y medible con límites de salida.
- Control directo sobre las memorias y su historial.
- Búsqueda textual local sin modelos de embeddings ni servicios de inferencia.

## 2. Límites de responsabilidad

| Componente | Responsabilidad |
|---|---|
| Mnexis | Persistencia, revisión, vigencia, autorización, recuperación y selección de memorias |
| Harness | Elegir cuándo consultar, insertar o retirar contexto; compactar historial; preservar llamadas y respuestas de herramientas |
| Modelo/agente | Formular consultas, proponer memorias, interpretar evidencia y detectar información faltante |
| Índice de código externo | Archivos, símbolos, dependencias y cambios del código |
| Usuario | Establecer permisos y corregir o aprobar conocimiento cuando corresponda |

Guardar una memoria **no libera tokens**. Mnexis no altera conversaciones de Pi, Codex ni otros clientes. `memory_context` es una selección de datos, no el contexto completo del agente. El harness debe contar también instrucciones, conversación, herramientas y reserva de respuesta.

### Fuera del MVP

Compactación/pruning, harness propio, extracción automática de todo el historial, subagentes, sincronización, SaaS, UI web, grafo temporal, detector automático de contradicciones, reranker neuronal y escaneo integral del código. No entrenar modelos ni garantizar la veracidad de las memorias.

## 3. Arquitectura Rust y stack propuesto

| Área | Decisión para el prototipo/MVP |
|---|---|
| Lenguaje y distribución | Rust estable fijado en `rust-toolchain.toml`; un ejecutable por plataforma |
| Persistencia | `rusqlite` con SQLite compilado mediante `bundled` |
| Archivo de datos | `mnexis.db` para datos canónicos y FTS5; WAL/SHM auxiliares de SQLite |
| Búsqueda textual | SQLite FTS5; verificar disponibilidad al iniciar y mediante CI |
| Búsqueda | SQLite FTS5; no se implementa búsqueda semántica mediante embeddings ni base vectorial |
| Runtime | Tokio; operaciones SQLite síncronas en trabajadores dedicados, sin bloquear hilos async |
| MCP | SDK oficial `rmcp`, transporte stdio; activar solo features necesarias |
| CLI | `clap`; mismo servicio de aplicación que MCP |
| Contratos | `serde`, `serde_json`, `schemars`; esquemas cerrados |
| Configuración y soporte | `toml`, `uuid`, `time`, `thiserror`, `tracing`, `tracing-subscriber` |
| Conteo de tokens | Adaptador interno; evaluar `tokenizers` para archivos locales compatibles, sin asumir soporte de todo proveedor |
| Pruebas | Cargo test, `tempfile`, benchmarks con `criterion`; evaluar `proptest` para invariantes |

Las versiones exactas se fijan tras comprobar compatibilidad, MSRV, licencias y compilación conjunta; incluir `Cargo.lock`. No usar rangos flotantes sin control ni depender de ramas main para release.

«Un binario» no significa datos ni pesos dentro del ejecutable. SQLite bundled incluye código C compilado: el producto es Rust, pero no exclusivamente Rust puro. El desarrollador necesita toolchain C compatible para compilar esas dependencias; el usuario recibe el ejecutable, sin instalar SQLite, Rust ni una extensión aparte. No prometer enlace completamente estático en todos los sistemas: comprobar bibliotecas de sistema requeridas en la matriz de distribución.

### Persistencia y búsqueda

SQLite es el único almacén de datos del MVP. Conserva memorias, revisiones, vigencia, permisos aplicados por el servicio y el índice textual FTS5. La búsqueda recupera candidatos por términos y filtros autorizados; no requiere embeddings, modelos de inferencia, LanceDB ni otra base vectorial. No hay outbox de indexación vectorial ni sincronización entre almacenes.

### Organización del código

Un paquete Cargo inicial con biblioteca y binario, módulos `domain`, `application`, `storage`, `retrieval`, `mcp`, `cli`, `config` y `diagnostics`. Dominio independiente de rmcp/rusqlite; adaptadores sin reglas de negocio. Evitar workspace con muchas crates hasta que exista necesidad real.

Interfaces internas pequeñas: repositorio de memorias y contador de tokens. No abstraer todos los posibles motores ni crear un framework. SQL parametrizado, migraciones numeradas incluidas en el binario; encapsular el adaptador SQLite y revisar cada bloque unsafe propio.

## 4. Alcance y prioridades

### P0 — MVP

1. Persistencia y revisión obligatoria; edición con control de concurrencia.
2. Política de ingesta documentada, identidad conceptual opcional y deduplicación exacta idempotente.
3. Estados de vigencia y sustitución explícita de decisiones.
4. Restricciones de operaciones y namespaces por proceso MCP.
5. Búsqueda textual FTS5 con filtros y degradación observable.
6. Contexto por presupuesto, procedencia y abstención conservadora.
7. Múltiples agentes sin pérdida de datos ni exposición de revisiones antiguas.
8. Borrado lógico, restauración y purga con garantías delimitadas.
9. Import/export, recuperación tras fallos y diagnóstico.
10. Evaluación reproducible y prueba con clientes reales.

### P1 — después de medir el MVP

Reranking opcional, sugerencias de duplicados y contradicciones revisables, integración de frescura con Git, resumen de sesiones con revisión, extensión específica de harness y otros adaptadores locales. Evaluar cada función por beneficio medido.

### P2

Relaciones explícitas entre memorias, UI y sincronización si existe demanda. No condicionan el diseño ni el lanzamiento del MVP.

## 5. Política de ingesta

El guardado es explícito: el usuario o agente llama una herramienta. Mnexis valida y persiste, pero no decide autónomamente qué merece recordarse.

| Información | Tratamiento recomendado al agente |
|---|---|
| Decisión adoptada y su motivo | Guardar, con proyecto y fuente |
| Preferencia explícita y duradera | Guardar con ámbito pertinente |
| Solución reutilizable y verificada | Guardar, indicando evidencia |
| Hipótesis o propuesta provisional | Guardar solo si aporta continuidad, en estado `proposed` |
| Resultado temporal, log largo o código completo | Conservar fuera de Mnexis; guardar conclusión y referencia si son útiles |
| Secreto o credencial | No guardar; el MVP no es una bóveda |
| Inferencia sobre el usuario | No presentar como confirmada; marcar `inferred` y aportar procedencia |

El contrato distingue `assertion_basis`: `user_stated`, `observed`, `inferred`, `imported`. Estos valores expresan origen de la afirmación, no probabilidades de verdad. Una inferencia no se convierte automáticamente en hecho porque se recupere muchas veces.

La procedencia mínima siempre incluye actor, canal de ingesta y fecha. Puede incorporar sesión, referencia externa, archivo, commit y fragmento de evidencia. Mnexis no lee ni ejecuta referencias al guardar.

## 6. Identidad, revisión y vigencia

### Identidad

- `id`: UUID estable del registro.
- `topic_key`: clave conceptual opcional y normalizada, única por namespace cuando existe; ejemplo `architecture.graph-engine`.
- `revision`: entero creciente, con historial obligatorio.
- `idempotency_key`: clave opcional por actor y namespace para reintentos de una escritura.

Una coincidencia de `topic_key` devuelve el ID existente y exige actualización explícita; nunca sobrescribe silenciosamente. Repetir una clave de idempotencia con contenido diferente devuelve conflicto. Sin `topic_key`, contenidos similares pueden coexistir; la coincidencia textual no autoriza fusionarlos.

### Estados de conocimiento

| Estado | Significado | Contexto predeterminado |
|---|---|---|
| `proposed` | Hipótesis o decisión pendiente | Excluido |
| `active` | Conocimiento vigente según la información registrada | Incluido si es pertinente |
| `disputed` | Conocimiento explícitamente cuestionado | Excluido |
| `superseded` | Sustituido por otro conocimiento | Excluido |
| `archived` | Historia conservada sin uso habitual | Excluido |

El borrado se representa aparte con `deleted_at` y `purge_after`.

### Cambios

`memory_update` exige `expected_revision`. En conflicto devuelve la revisión actual y no escribe. Cada cambio conserva contenido y metadatos anteriores en `memory_history` dentro de la transacción.

Actualizar el mismo concepto crea una revisión del mismo ID. Sustituir una memoria por otra de identidad distinta usa `memory_supersede`: activa la nueva, marca la antigua como `superseded` y enlaza ambas de forma atómica. Exige autorización sobre ambas y, en MVP, el mismo namespace. Rechaza ciclos.

`valid_from` y `valid_until` son opcionales, expresados en UTC. La recuperación vigente respeta esos límites. El MVP no ofrece consultas bi-temporales completas. La recencia por sí sola no determina verdad: una preferencia antigua puede seguir vigente.

## 7. Ámbitos y permisos

Cada proceso MCP se inicia con configuración de ámbito controlada por el usuario, fuera de los argumentos de las herramientas:

- Namespaces permitidos para lectura y escritura.
- Operaciones permitidas; purga, importación y configuración son CLI administrativa en MVP.
- Namespace predeterminado obligatorio.
- Permiso separado para incluir `global`; no se añade implícitamente.
- Las lecturas pueden abrirse entre proyectos (`allow_read_all`, activo por
  defecto en los perfiles nuevos): el agente necesita ver las memorias de
  otros proyectos porque los proyectos comparten componentes. Las
  escrituras no se abren: un proyecto escribe sólo en su namespace, y el
  CLI rechaza con `FORBIDDEN` la escritura a otro namespace cuando el
  workspace actual está vinculado a uno.

`all` significa **todos los namespaces autorizados para ese proceso**, nunca todo el perfil. Una herramienta no puede ampliar su ámbito pasando un `client_id` o modificando parámetros. Solicitudes fuera del ámbito se rechazan sin revelar contenido ni existencia de IDs.

Todas las rutas aplican autorización: búsqueda, contexto, get, list, historial, actualización, sustitución, borrado y restauración. También se filtran consultas por IDs y cursores. Los límites se resuelven antes de recuperar candidatos y se revalidan al construir la salida.

**Límite de seguridad:** estas restricciones protegen el acceso por herramientas MCP; no aíslan un agente con permisos de shell sobre los mismos archivos del usuario. Para ese caso se necesita sandbox o separación a nivel de sistema operativo. No presentar namespaces como una frontera contra procesos locales hostiles.

## 8. Contrato de herramientas y CLI

Todos los contratos son versionados, usan esquemas cerrados y límites explícitos. No exponen SQL arbitrario.

| Herramienta MCP | Operación y entradas principales |
|---|---|
| `memory_save` | Contenido, tipo, namespace, procedencia, estado y claves opcionales |
| `memory_update` | ID, `expected_revision`, cambios y motivo |
| `memory_supersede` | IDs anterior/nuevo, revisiones esperadas y motivo |
| `memory_search` | Consulta, ámbito, filtros, modo, límite y cursor |
| `memory_context` | Tarea, ámbito, presupuesto, tokenizer y tipos/filtros |
| `memory_get` | ID y, opcionalmente, revisión autorizada |
| `memory_list` | Filtros, límite y cursor |
| `memory_history` | ID, límite y cursor; revisiones obligatorias |
| `memory_delete` | Borrado lógico e ID/revisión esperada |
| `memory_restore` | Restauración antes de purga, sin activar ciegamente conocimiento sustituido |

Comandos: `version`, `init`, `mcp serve`, `save`, `update`, `supersede`, `search`, `context`, `list`, `show`, `history`, `delete`, `restore`, `purge`, `export`, `import`, `reindex`, `doctor`, `config` y `eval`.

La CLI administrativa opera bajo autoridad del usuario; para pruebas y automatización permite imponer el mismo ámbito restringido de MCP. Los clientes objetivo y la forma real de conectar MCP se verifican mediante pruebas: no asumir que todos soportan stdio de manera nativa.

### Límites iniciales, configurables dentro de máximos seguros

Contenido: 16 KiB UTF-8 por memoria; consulta/tarea: 8 KiB; tags: 32; resultados: 20 por defecto y 100 máximo; contexto: 20 memorias máximo además del presupuesto; importación: 50 MiB y 100.000 registros. Las respuestas tienen límite adicional de bytes. Validar límites antes de trabajo costoso. Una memoria demasiado extensa requiere dividirla explícitamente; no se fragmenta en silencio.

Paginación por cursor opaco ligado a filtros y ámbito; list usa orden estable por fecha e ID. Search pagina candidatos de una ejecución acotada y no promete estabilidad frente a cambios posteriores; cursor expirado exige nueva búsqueda.

Errores estables: `INVALID_ARGUMENT`, `NOT_FOUND`, `FORBIDDEN`, `REVISION_CONFLICT`, `TOPIC_CONFLICT`, `BUSY`, `INDEX_UNAVAILABLE`, `BUDGET_TOO_SMALL`, `CURSOR_EXPIRED` y `UNSUPPORTED_VERSION`. No incluir texto de memorias en logs de error.

## 9. Recuperación orientada a tareas

### Pipeline

1. Validar y resolver autorización, namespace y estado vigente.
2. Obtener candidatos mediante FTS5.
3. Aplicar filtros autorizados al conjunto candidato antes de seleccionar resultados.
4. Revalidar en SQLite ID, revisión, estado y vigencia.
5. Aplicar política conservadora de elegibilidad y diversidad.
6. Seleccionar memorias completas dentro del presupuesto de salida.

Los límites de candidatos son configurables y sujetos a evaluación. La puntuación textual es una señal de ordenamiento, **no una probabilidad de relevancia**. No ofrecer `min_relevance` sin calibración.

La selección evita repetir el mismo `topic_key` y elimina duplicados exactos. El MVP usa una heurística determinista y explicable de utilidad/costo; no promete encontrar el subconjunto matemáticamente óptimo.

### Contexto y tokens

```json
{
  "task": "Optimizar el renderizado del grafo",
  "namespace": "proyecto-a",
  "token_budget": 2500,
  "tokenizer_id": "supported-tokenizer-v1",
  "memory_types": ["decision", "fact"],
  "max_items": 12
}
```

El identificador es ilustrativo; el spike debe fijar y publicar los tokenizers realmente soportados. Se cuenta el texto completo serializado de la respuesta, incluidas procedencia y estructura. Para tokenizer soportado, la salida no excede el presupuesto. Si no cabe ninguna memoria, devuelve selección vacía o `BUDGET_TOO_SMALL` cuando ni la envoltura mínima cabe.

Sin tokenizer soportado, se permite modo explícito de estimación con `budget_enforcement: estimated` y límite duro en bytes; no se afirma una garantía exacta de tokens. El cliente puede exigir conteo exacto y rechazar dicho modo. No truncar frases ni generar resúmenes automáticamente en MVP.

La salida incluye `items`, `retrieval_mode`, `budget_enforcement`, `tokens_used` o estimación, `tokenizer_id`, `index_status`, `warnings` y `empty_reason` cuando corresponda. Cada item incluye ID, revisión, namespace, contenido, tipo, estado y procedencia. `search --explain` muestra rankings, filtros y motivos de exclusión sin revelar candidatos no autorizados.

### Abstención y degradación

Es válido devolver cero memorias. La búsqueda textual usa coincidencias FTS5 y filtros autorizados; no realiza búsqueda semántica ni presenta candidatos vectoriales. La abstención reduce ruido, pero no certifica irrelevancia. Documentar falsos negativos. El modo de recuperación es textual; los fallos o limitaciones de FTS5 se informan sin degradar silenciosamente el resultado.

## 10. Persistencia y esquema conceptual

### Tablas canónicas

| Tabla | Datos principales |
|---|---|
| `memories` | ID, namespace, topic_key, tipo, contenido, estado, revisión, vigencia, procedencia, fechas y borrado |
| `memory_history` | Revisiones anteriores completas y motivo/actor del cambio |
| `memory_links` | Relación `supersedes`, sin grafo general en MVP |
| `memory_tags` | Tags normalizados para filtros |
| `idempotency_records` | Clave, hash de solicitud y resultado, con retención documentada |
| `schema_migrations` | Versiones y migraciones aplicadas |

FTS5 se actualiza mediante triggers o servicio transaccional escogido en el spike. El índice textual operativo contiene memorias no borradas; el estado se filtra al recuperar. Historial no se incluye en búsquedas vigentes. Cada operación de guardar, editar, borrar o restaurar actualiza SQLite y FTS5 en una sola transacción SQL.

### Consistencia y recuperación

Guardar, editar, borrar y restaurar son operaciones transaccionales en SQLite, incluyendo la actualización de FTS5. No hay workers, reindexación vectorial, generaciones de índices ni consistencia entre almacenes. `doctor` verifica el estado de SQLite y FTS5.

### Índice textual

FTS5 se mantiene junto con los datos canónicos mediante transacciones SQLite. No existen generaciones de modelos ni índices vectoriales que deban reconstruirse o activarse.

## 11. Concurrencia entre agentes

Varios procesos MCP/CLI operan un perfil en disco local. SQLite WAL, foreign_keys y busy_timeout por conexión; un escritor SQL a la vez, transacciones cortas y control optimista. Consultas síncronas rusqlite se ejecutan en trabajadores dedicados/capacidad limitada, sin mutex ni transacción atravesando await.

BUSY tiene timeout y diagnóstico. Si falla SQLite canónico, devolver error, nunca confirmar escrituras. No soportar perfil abierto en carpetas de red ni sincronización activa de archivos; exportar para trasladarlo.

**Gate:** tres procesos con save/update/delete concurrentes. Comprobar ausencia de pérdida, duplicados visibles y conocimiento borrado/obsoleto devuelto. No introducir daemon obligatorio sin revisar alcance.

## 12. Borrado y retención

### Borrado lógico

En una transacción SQLite se marca el registro y se retira de FTS5. Se conserva el registro completo y su historial en SQLite para restaurar durante 30 días, configurable incluso a cero. No se llama a esto eliminación definitiva.

### Restauración

Solo antes de la purga. Restaura el estado previo cuando sigue siendo válido; si existe una sustitución o conflicto conceptual posterior, devuelve conflicto y requiere decisión explícita. Reindexa la revisión vigente.

### Purga

Elimina contenido canónico, revisiones, tags, referencias con contenido e información derivada; conserva solo eventos técnicos mínimos sin texto cuando sean necesarios. El comando administrativo informa pendientes/fallos.

El vencimiento hace elegible una memoria para purga. No existe temporizador permanente: se procesa durante actividad/mantenimiento; la fecha no promete ejecución con la aplicación cerrada.

La purga garantiza eliminación lógica de datos administrados; **no garantiza borrado forense** en SSD, snapshots, WAL histórico o copias externas. Exports y backups previos quedan bajo control del usuario y deben eliminarse por separado. No crear un backup con contenido antes de una purga solicitada.

## 13. Importación, exportación y migraciones

Formato JSONL versionado: manifiesto y registros tipados, con esquema, límites y checksums para detectar corrupción accidental. Un checksum no autentica el origen.

Exportación usa snapshot SQL consistente; exporta memorias y revisiones seleccionadas, sin necesitar copiar el índice reconstruible. Por defecto excluye borradas; modo histórico y contenido recuperable requieren selección explícita. No ampliar autorización para exportar.

Importación administrativa ofrece `--dry-run`, informe de conflictos y transacción atómica dentro de límites. No descarga referencias ni ejecuta hooks, SQL, código o comandos contenidos en datos.

- ID idéntico y payload canónico idéntico: no-op.
- ID idéntico con diferencias: conflicto; política por defecto `reject`.
- `topic_key` ocupado con otro ID: conflicto explícito.
- Modo `remap`: genera IDs nuevos y remapea enlaces internos; sigue rechazando claves conceptuales ocupadas.
- No admitir reemplazo masivo silencioso ni merge automático de revisiones en MVP.

Un round-trip a una base vacía conserva IDs, estados, contenido, revisiones, metadatos y fechas. El índice FTS5 se actualiza a partir de los datos importados; no se importan índices externos. Validar todo antes de escribir, con máximo de expansión si en el futuro se aceptan paquetes comprimidos; MVP no requiere archivos comprimidos.

Migraciones versionadas bajo exclusión; backup coherente antes de migraciones destructivas, sujeto a política de retención documentada. Preservar archivos incompatibles y diagnosticar antes de reconstruir un índice. No ejecutar migraciones simultáneas.

## 14. Seguridad y operación local

- MCP usa stdout exclusivamente para protocolo; logs van a stderr, sin contenido por defecto.
- Memorias y referencias son datos no confiables. `kind=instruction` no las eleva a instrucciones del sistema; el harness debe separarlas y no ejecutar órdenes embebidas.
- Esquemas cerrados, SQL parametrizado y validación de tipos, tamaños, rutas y filtros.
- Directorios con permisos restrictivos cuando el SO lo permita; no afirmar cifrado en reposo.
- Sin telemetría, actualizaciones ni descargas automáticas. Probar ausencia de egress en save/search/doctor.
- Timeouts y cancelación de solicitudes; una operación bloqueada no impide otras consultas independientes.

Datos bajo directorio de plataforma; precedencia `--data-dir` > `MNEXIS_HOME` > valor estándar. Configuración legible separada; `config show` muestra rutas efectivas. Builds objetivo: Linux amd64/arm64, macOS arm64, Windows amd64, sujetos a validación.

`doctor` revisa integridad SQL y FTS5 con un contrato estable por check (`check_id`, `severity`, `reason_code`, `evidence`, `safe_next_step`; `--check CODE` filtra). `doctor repair --plan|--dry-run|--apply` es explícito, mide antes de escribir y distingue tareas reconstruibles de daños canónicos. Logs agregados: latencia, modo, conteos y fallos, sin consultas ni contenido salvo diagnóstico local opt-in.

## 15. Evaluación y aceptación

Los umbrales siguientes son **objetivos iniciales del producto**, no mediciones. Se congelan antes de comparar implementaciones; si se revisan, registrar motivo y resultados previos.

### Corpus y metodología

Fixture versionado mínimo: 500 memorias, 100 consultas relevantes y 50 sin respuesta, distribuidas entre proyectos y en español/inglés. Incluir identificadores, paráfrasis, duplicados, sustituciones, errores de origen y consultas históricas. Separar conjuntos de ajuste y evaluación; no calibrar sobre el conjunto final.

Evaluar la búsqueda FTS5 con el mismo contenido y filtros del corpus. Comparación con Engram: versión/configuración fijadas, mismo corpus importable y funciones comparables; diferencias de autorización/vigencia se reportan aparte. No modificar el baseline para favorecer Mnexis.

Medir Recall@5, MRR@5, Precision@5, falsos positivos en consultas sin respuesta, recuperación de conocimiento obsoleto, tokens, latencia p50/p95, RSS y disco. Evaluar además al menos 20 tareas de agente con/sin memoria bajo modelo y presupuesto fijos, registrando corrección, errores por memoria y costo; no deducir éxito del agente solo de recall.

### Criterios verificables

| ID | Criterio de aceptación P0 |
|---|---|
| A-01 | Binario inicia e inicializa perfil sin toolchain Rust ni servidor SQL en cada plataforma anunciada |
| A-02 | Reiniciar conserva memorias y todas sus revisiones |
| A-03 | Dos updates sobre la misma revisión: uno confirma y el otro recibe conflicto, sin pérdida |
| A-04 | Ninguna ruta MCP devuelve contenido fuera del ámbito configurado, incluyendo IDs y cursores manipulados |
| A-05 | Contexto predeterminado excluye propuestas, disputadas, sustituidas, archivadas, vencidas y borradas |
| A-06 | Sustitución e historial son atómicos; crash no deja estados intermedios visibles |
| A-07 | Contexto con tokenizer soportado nunca supera presupuesto serializado; modo estimado se etiqueta |
| A-08 | Recall@5 objetivo ≥ 0,85 y MRR@5 ≥ 0,70 en evaluación congelada de búsqueda textual |
| A-09 | Consultas sin respuesta reciben memorias en ≤ 10% de casos; reportar simultáneamente recall para evitar abstención trivial |
| A-10 | Cero filtraciones y cero memorias explícitamente sustituidas como vigentes en fixtures; no implica garantía universal de verdad |
| A-11 | FTS5 no disponible o con error se informa explícitamente; no se presentan resultados como completos |
| A-12 | Un fallo durante save/update/delete no deja cambios parciales en datos canónicos ni en FTS5 |
| A-13 | El índice FTS5 puede reconstruirse desde datos canónicos tras interrupción sin pérdida de memorias |
| A-14 | Tres procesos, escrituras/lecturas concurrentes: sin pérdida ni revisiones obsoletas expuestas |
| A-15 | El índice textual refleja las revisiones vigentes después de update, delete y restore |
| A-16 | Borrado excluye inmediatamente la salida; restore/purge/rebuild respetan retención y vigencia |
| A-17 | Round-trip conserva contenido canónico; import malicioso y conflictos fallan sin escritura parcial |
| A-18 | Pruebas de red no observan egress externo; logs predeterminados no contienen memorias |
| A-19 | Un agente de referencia y un segundo cliente realizan el flujo MCP; documentar adapters necesarios |
| A-20 | Primera memoria en < 10 minutos con el binario disponible; medir tiempo de configuración |

Rendimiento: benchmark con 10.000 y 100.000 memorias en equipo declarado, consultas frías/calientes y longitud registradas. El spike fija presupuestos numéricos de latencia, RAM y disco antes de cerrar diseño. No aprobar release dejando esos presupuestos indefinidos.

## 16. Plan de entrega y gates

### Fase 0 — contratos y evaluación

Cerrar esquemas, estados, scopes, presupuesto y fixture. Probar manualmente escenarios de decisiones sustituidas, conflicto de revisión, consulta sin respuesta y dos agentes.

**Salida:** contratos versionados y criterios congelados para búsqueda textual.

### Fase 1 — spike técnico

Rust/rusqlite/FTS5, rmcp stdio, builds, concurrencia SQLite, crash/reopen y presupuesto de recursos.

**Salida:** decisión arquitectónica con resultados de calidad y operación para búsqueda textual.

### Fase 2 — núcleo durable y CLI

Revisiones, estados, permisos, FTS5, export/import y borrado. Pruebas de fallos y concurrencia desde esta fase.

**Salida:** datos canónicos confiables y administración independiente del agente.

### Fase 3 — MCP y contexto

Herramientas, scopes, tokenizer, abstención, diagnóstico y pruebas reales de clientes. Documentar instrucciones de uso y frontera con el harness.

**Salida:** flujo completo de memoria recuperable sin modificar el historial del agente.

### Fase 4 — validación y release

Benchmark, tareas de agente y usuarios piloto. Congelar esquema/formato publicados, licencia, matriz de plataformas y migración. No sumar UI o automatización para compensar una recuperación deficiente.

**Salida:** criterios P0 aprobados o limitaciones explícitas que reduzcan el alcance anunciado.

## 17. Riesgos y decisiones pendientes

| Riesgo | Validación o respuesta |
|---|---|
| Backend embebido inseguro entre procesos | Prueba multiproceso; cambiar candidato o almacenamiento |
| Memorias incorrectas o envejecidas | Procedencia, estados y revisión; no prometer verificación automática |
| Búsqueda textual no encuentra paráfrasis | Medir falsos negativos y comunicar claramente el alcance léxico de FTS5 |
| Contexto barato pero insuficiente | Evaluar tareas, recall y errores junto con tokens |
| Autoridad MCP confundida con aislamiento de SO | Documentar frontera y prueba de scopes |
| Borrado interpretado como borrado forense | Separar lógico, purga y copias externas |
| La recuperación textual resulta insuficiente | Revisar evidencia de uso antes de proponer cambios de alcance; la búsqueda semántica vectorial queda fuera de la decisión actual |
| Exceso de alcance | Entregar búsqueda textual y control manual antes de automatización inteligente |

Pendientes del spike: licencias de dependencias, tokenizers soportados, límites de recursos y clientes concretos. La licencia del producto y el nombre se resuelven antes de publicación.

## 18. Cambios respecto del PRD anterior

- Decisión vigente: SQLite con FTS5 es el único almacén y mecanismo de búsqueda del MVP; no se usarán embeddings, base de datos vectorial ni búsqueda semántica vectorial.
- Se reemplaza el máximo fijo de cinco recuerdos por selección dentro de presupuesto y límite de items.
- Se añade política de ingesta, procedencia estructurada, identidad conceptual y vigencia explícita.
- El historial deja de ser opt-in; las actualizaciones requieren revisión esperada.
- Se precisan autorización por proceso y sus límites ante acceso directo al sistema.
- Se permite concurrencia y se exige validarla antes de fijar el backend.
- Se distinguen exclusión inmediata, restauración y purga.
- Se mantiene FTS5 junto a los datos canónicos en SQLite; se eliminan los estados de indexación vectorial y las generaciones de modelos de la propuesta anterior.
- Se sustituye el único fixture top-5 por evaluación de recuperación y tareas reales.
- El tipo `session_summary` existe desde P0 como dato manual; su generación automática queda en P1.
- Reranking, detección automática de contradicciones e integración con Git quedan fuera del MVP.

## 19. Referencias internas preservadas

El original remite a `propuesta.md`, `cognee.md`, `engram.md`, `graphiti.md` y `mem0.md`. No se adjuntaron esos documentos; se conservan como referencias pendientes de validación, sin usarlos como evidencia de capacidades actuales de terceros.

## 20. Fuentes técnicas y validaciones del stack

Fuentes primarias consultadas para la propuesta Rust el 13 de octubre de 2026. Documentan capacidades de dependencias, no resultados de Mnexis:

- [rusqlite: integración y compilación bundled](https://github.com/rusqlite/rusqlite).
- [SQLite: búsqueda textual FTS5](https://www.sqlite.org/fts5.html).
- [SQLite: WAL y límites de concurrencia](https://www.sqlite.org/wal.html).
- [MCP: SDK oficial Rust rmcp](https://github.com/modelcontextprotocol/rust-sdk).
- [Tokio: trabajo síncrono con spawn_blocking](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html).
- [Hugging Face Tokenizers](https://github.com/huggingface/tokenizers).

Antes de release: fijar versiones, verificar licencias transitivas, ejecutar cargo fmt, clippy y pruebas; compilar por plataforma y probar en entorno limpio. El objetivo no es prometer más velocidad que Go: se valida el rendimiento del producto completo.

## 21. Decisión de búsqueda

El MVP y el producto descrito por este PRD usan SQLite FTS5 para búsqueda textual. Quedan fuera embeddings, modelos de embedding, bases vectoriales (incluido LanceDB) y búsqueda semántica vectorial. Los informes de prototipo previos se conservan únicamente como evidencia histórica y no representan requisitos vigentes.

## 22. Ruta ejecutable hasta el MVP

Las etapas se completan por evidencia, no por fechas estimadas. Mantener una lista de trabajo ligada a los criterios A-01 a A-20. El MVP final incluye recuperación textual validada con FTS5.

| Etapa | Depende de | Entregable | Condición para continuar |
|---|---|---|---|
| E0 — Definir contratos | Ninguna | Esquemas, fixtures, decisiones de ámbito y revisiones | Escenarios y errores definidos antes del código |
| E1 — Validar stack | E0 | Spike Rust/SQLite/FTS5/rmcp | Builds y operaciones locales básicas comprobadas |
| E2 — Núcleo SQL usable | E1 | CLI, memorias, historial, vigencia y FTS5 | Reinicio, conflictos, permisos y borrado correctos |
| E3 — Recuperación útil | E2 | Búsqueda textual, abstención y presupuesto | Calidad y límites pasan fixtures congelados |
| E4 — Uso por agentes | E3 | MCP, configuración y guía | Dos clientes completan flujo real |
| E5 — Preparar release | E4 | Export/import, repair, paquetes y evidencia | Todos los criterios P0 revisados y aprobados |

### E0. Contratos y escenarios

1. Definir structs de memoria, procedencia, estados, resultados y errores.
2. Definir revisión esperada, topic_key, idempotency_key y sustitución.
3. Establecer scopes por proceso, incluyendo global/all y operaciones administrativas.
4. Preparar fixture mínimo y separar ajuste/evaluación. Añadir casos sin respuesta y decisiones sustituidas.
5. Escribir decisiones cortas: SQLite/FTS5 como único almacén y búsqueda, y responsabilidad del harness.

**Cierre:** ejemplos completos de save/update/search/context/delete y comportamiento de conflicto; no quedan ambiguos los permisos ni la vigencia.

### E1. Spike acotado de integración

1. Crear paquete Rust con biblioteca/binario, toolchain y lockfile.
2. Abrir SQLite bundled y verificar FTS5.
3. Insertar, consultar, actualizar y borrar registros; cerrar y reabrir SQLite.
4. Exponer una herramienta MCP mínima mediante rmcp, con stdout limpio.
5. Ejecutar con dos procesos y probar consistencia de lectores SQLite; medir compilación, RSS, disco y arranque en plataformas objetivo.
6. Medir búsqueda FTS5 y fijar presupuestos de recursos antes de avanzar a release.

**Cierre:** informe reproducible del stack y versiones elegidas. Si falla una garantía de SQLite/FTS5, registrar decisión y corregir el diseño antes de construir sobre ella.

### E2. Núcleo durable y CLI textual

1. Implementar migraciones y repositorio SQL con transacciones cortas.
2. Save/get/list/update/history/supersede con validación y revisión optimista.
3. Implementar scopes en el servicio de aplicación, no solo en MCP.
4. Mantener FTS5 en cada transacción y recuperar solo estados vigentes por defecto.
5. Añadir borrado lógico, restauración y administración básica del perfil.
6. Implementar comandos CLI correspondientes y errores estables.

**Pruebas de cierre:** persistencia tras reinicio; updates simultáneos; historial completo; namespaces e IDs no autorizados; sustituciones; borrado y restauración. Esta etapa permite usar memoria textual de forma independiente.

### E3. Recuperación textual y selección de contexto

1. Implementar búsqueda FTS5 con filtros autorizados.
2. Implementar explicación de resultados y señal de errores/cobertura.
3. Calibrar abstención usando únicamente conjunto de ajuste.
4. Seleccionar memorias completas por costo/diversidad; contar respuesta serializada con tokenizer soportado.
5. Evaluar búsqueda textual sobre conjunto congelado.

**Cierre:** criterios de búsqueda y presupuesto aprobados; métricas publicadas.

### E4. MCP y prueba de uso real

1. Exponer herramientas completas sobre el servicio existente, sin duplicar lógica.
2. Configurar ámbitos por proceso; impedir que llamadas amplíen permisos.
3. Documentar instalación, herramientas y límites de la búsqueda textual.
4. Probar agente de referencia y segundo cliente; documentar puente/adaptador si lo necesitan.
5. Ejecutar tareas reales con/sin memoria: decisión previa, corrección, sustitución, recuperación siguiente sesión y tarea sin memoria pertinente.

**Cierre:** dos clientes guardan/recuperan/corrigen/borran; tres procesos comparten perfil sin pérdida ni fugas. Verificar que la memoria ayuda y no introduce decisiones antiguas como actuales. El harness sigue administrando el historial.

### E5. Operación, portabilidad y release MVP

1. Completar import/export versionado con dry-run y conflictos explícitos.
2. Completar doctor/repair y purga.
3. Probar ausencia de egress externo y contenido sensible en logs.
4. Automatizar fmt, clippy, tests, builds y smoke tests por plataforma anunciada.
5. Validar instalación en entorno limpio, backup/migración coherente y recuperación.
6. Publicar README, configuración MCP, guía CLI, límites de seguridad y resultados del benchmark.
7. Revisar A-01 a A-20: cada criterio tiene evidencia, responsable y resultado. No usar un promedio para esconder fallos de integridad o autorización.

**Cierre MVP:** flujo completo reproducible, ninguna pérdida canónica en fallos probados, recuperación dentro del presupuesto, datos administrables y plataformas anunciadas verificadas. Si un criterio se elimina para reducir alcance, actualizar el PRD y la descripción pública antes de release.

### Primer bloque de trabajo para el agente de implementación

Empezar con E0 y E1. Producir contratos, spike ejecutable e informe de decisiones. Después implementar E2; evitar construir todas las herramientas, UI y sincronización de una vez. Cada etapa termina con una revisión del resultado y evidencia de sus criterios antes de ampliar alcance.

### Qué no agregar antes del MVP

UI, sincronización entre dispositivos, grafo de memorias, extracción automática de conversaciones, reranking neuronal, inferencia embebida fastembed, proveedores cloud y pruning del harness. Se evalúan después del flujo durable y de la recuperación medida.
