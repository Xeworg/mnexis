# project-isolation — escrituras acotadas al proyecto actual, lecturas abiertas entre proyectos

El agente que trabaja en un proyecto sólo escribe en el namespace de ese
proyecto y nunca modifica memorias de otro; a la vez puede leer memorias de
otros proyectos por defecto, porque el usuario trabaja en proyectos que
comparten componentes.

## Specs

- S1: "el agente debe sabr que si esta en proyecto debe solo agregar memorias
  a ese bajo ningun motivo puede modificar memorias de otro" — mientras
  trabaja en un proyecto, el agente sólo agrega memorias al namespace de ese
  proyecto y no modifica memorias de otro proyecto.
- S2: "pero si me gustaria que pueda ver memorias de otro proyectos por
  defecto ya que el usuario podria estar trabajando en proyectos que oucupen
  componentes de otro" — las lecturas sí cruzan proyectos por defecto.
- S3: "allow_read_all = true" — el profile tiene una clave nueva
  `allow_read_all`; con ella activa, `can_read` vale para cualquier
  namespace, salvo el literal reservado `"global"`, que sigue rigido por
  `allow_global`.
- S4: "allow_global = false" — `mnexis init` escribe `allow_global = false`:
  el namespace reservado `"global"` no se puede leer ni escribir hasta que el
  humano lo habilite explícitamente.
- S5: "Duro en el CLI" — el CLI resuelve el namespace del workspace actual
  (binding del proyecto por clave de workspace, o marker
  `.mnexis-namespace`) y rechaza toda escritura cuyo namespace destino sea
  otro:

  ```
  cd ~/Proyectos/mnexis   (bound -> "mnexis")
  $ mnexis save --namespace otro-proyecto "x"
    -> FORBIDDEN: este workspace escribe sólo en 'mnexis'

  cd ~   (sin binding)
  $ mnexis save --namespace otro-proyecto "x"
    -> permitido si está en la allowlist
  ```

  Cuando el cwd no resuelve a ningún namespace, manda la allowlist del
  profile (comportamiento actual, sin cambios).
- S6: con `allow_read_all` activo, `mnexis search`, `mnexis list` y
  `mnexis show` alcanzan memorias de otros namespaces; la extensión de pi
  lee por defecto entre proyectos y no sólo en su namespace.
- S7: las herramientas de escritura de la extensión quedan acotadas al
  namespace del workspace: guardar no acepta otro namespace y supersede
  rechaza un objetivo que viva fuera de ese namespace.
- S8: el agente tiene que saber la regla: el protocolo de la skill y las
  descripciones de las tools de la extensión la enuncian (escribir sólo en el
  proyecto propio; leer entre proyectos está permitido).
- S9: la regla se aplica en código, no sólo en el prompt: un rechazo por
  aislamiento de escritura devuelve `FORBIDDEN` (exit 4) con un mensaje que
  nombra el namespace permitido y cómo cambiarlo.
- S10: los profiles existentes no se reescriben: conservan lo que su
  `mnexis.toml` dice; `mnexis config allow-global [--revoke]` y
  `mnexis config allow-read-all [--revoke]` permiten cambiar los dos
  permisos sin editar el TOML a mano.

## Tasks

| ID | Spec | Route | Commit |
|----|------|-------|--------|
| T1 | S3, S4, S10 | Scope y profile: clave `allow_read_all`, `allow_global` por defecto en false, `can_read` con lectura abierta salvo `"global"`, comandos `config allow-global` / `config allow-read-all` (con `--revoke`), `config show`, `scope.schema.json` | a639e30 |
| T2 | S6 | Lecturas abiertas: la extensión lee entre proyectos por defecto (search/list sin namespace, show por UUID) y el CLI las sostiene | 7dfeeb3 |
| T3 | S5, S9 | Aislamiento de escritura en el CLI: resolución del namespace del workspace actual (binding por clave, marker `.mnexis-namespace`) y guard de escritura en save/update/supersede/delete/restore | 559668a, da9f12a |
| T4 | S7, S8 | Extensión acotada al proyecto: supersede rechaza objetivos fuera del namespace del workspace; descripciones de las tools y protocolo de la skill | ac03d95 |
| T5 | S3, S5, S6 | Documentación: README, `docs/`, contrato v1 | |
| T6 | S1, S2, S5, S9 | Verificación independiente del cambio de permisos + suite completa | |

## Log

- L1: "explicame el punto uno la descision mia" — el usuario pidió entender la
  decisión abierta sobre `allow_global` antes de resolverla.
- L2: el usuario eligió en la pregunta abierta: "el agente debe sabr que si
  esta en proyecto debe solo agregar memorias a ese bajo ningun motivo puede
  modificar memorias de otro, pero si me gustaria que pueda ver memorias de
  otro proyectos por defecto ya que el usuario podria estar trabajando en
  proyectos que oucupen componentes de otro" — de acá salen S1 y S2.
- L3: el usuario eligió "allow_read_all = true" para las lecturas (S3) con el
  preview `allow_global = false` (S4).
- L4: el usuario eligió "Duro en el CLI" para el aislamiento de escritura,
  con el preview quoted en S5.
- L5: contexto heredado de la sesión anterior: `mnexis init` creaba el
  profile con `allow_global = true`, lo que dejaba el namespace reservado
  `"global"` escribible de fábrica y en tensión con
  `docs/PRD-Mnexis.md:155` ("Permiso separado para incluir `global`; no se
  añade implícitamente") y con `docs/contracts/v1/README.md:11`.
- L6 (evidencia, 2026-10-21): el aislamiento de escritura hoy no existe.
  `src/application/service.rs:269` y `:380-381` autorizan update/supersede
  contra la allowlist del profile, así que un namespace autorizado en otro
  momento sigue siendo escribible desde otro proyecto; y la extensión
  autoriza el namespace del workspace (`extensions/mnexis-memory/lib/gate.js:52-68`)
  mientras `supersede`/`update` trabajan por UUID, sin comprobar el namespace
  del objetivo.
- L7 (evidencia, 2026-10-21): la lectura entre proyectos está a medias. El
  CLI cruza namespaces cuando no se pasa `--namespace`
  (`src/application/service.rs:646-657`), pero la extensión pasa siempre
  `namespace: workspace.namespace` (`extensions/mnexis-memory/index.js:312`
  y `:345`), y `mnexis show <uuid>` de otro namespace falla con FORBIDDEN
  porque `read_namespaces` es `["default"]` (`src/config/settings.rs:46-47`).
- L8 (T1, RED→GREEN): `can_read` con `allow_read_all` abre cualquier
  namespace salvo el literal reservado `"global"`, que sigue rigido por
  `allow_global`; `with_read_all` deja el constructor explícito por
  defecto en `false`. El profile se parsea sobre una base cerrada
  (`ProfileSettings::file_base`: `allow_global = false`,
  `allow_read_all = false`), así que un profile anterior a la clave no
  abre sus lecturas solo; `init` sí escribe `allow_read_all = true` y
  `allow_global = false`. Comandos nuevos: `mnexis config allow-global
  [--revoke]` y `mnexis config allow-read-all [--revoke]`, que exigen un
  profile existente (exit 3) y no tocan las allowlists de namespaces.
- L9 (T1, contrato): `allow_read_all` queda **opcional** en
  `docs/contracts/v1/scope.schema.json` (no entra en `required`): los
  fixtures E0 congelados siguen siendo válidos y el schema sigue
  rechazando la clave si aparece con otro tipo. La explicación del
  permiso va en `docs/contracts/v1/README.md` (T5).
- L10 (T1, evidencia): suite completa en verde (143 lib + 1 bin + 193
  integración), `cargo fmt --check` limpio, clippy en 36 warnings (los
  mismos de antes del cambio). Dos tests existentes fijaban el
  comportamiento viejo y se re-pinzaron: el de MCP que negaba un
  namespace fuera de scope ahora usa un profile con
  `allow_read_all = false` (la negación se sigue probando) y se agregó el
  caso nuevo de lectura abierta.
- L11 (T2, RED→GREEN): la extensión deja de acotar las lecturas a su
  namespace: `mnexis_search` y `mnexis_list` no pasan `--namespace` salvo
  que el modelo lo pida explícitamente, y sus descripciones lo dicen. Las
  escrituras no cambian: `mnexis_save` sigue apuntando al namespace del
  workspace.
- L12 (T2, hueco encontrado): `allow_read_all = false` no alcanzaba para
  acotar las lecturas, porque `search` y `list` sin namespace no
  chequeaban el scope y cruzaban todos los namespaces. Ahora el scope
  decide: `AccessScope::read_scope()` devuelve `All`, `AllExcept(["global"])`
  o `Only(read_namespaces)`, y el puerto del repositorio acepta ese
  filtro (`list_in`, `search_in`) con la misma semántica en SQL
  (`namespace IN (...)`, `namespace NOT IN (...)`). El literal reservado
  `global` queda fuera de las lecturas abiertas mientras `allow_global`
  no lo permita.
- L13 (T2, evidencia): 144 pruebas de lib, 51 de `cli_commands` (las dos
  nuevas: un profile acotado no busca fuera de sus namespaces, uno
  abierto sí), 35 pruebas de node (4 nuevas en
  `extensions/mnexis-memory/test/tools.test.js`, que manejan la extensión
  con un `mnexis` falso que registra sus argumentos), clippy en 36
  warnings.
- L14 (T4, RED→GREEN): `mnexis_supersede` comprueba que las dos memorias
  viven en el namespace del workspace antes de tocar nada; si una no se
  puede leer, tampoco se toca (no se puede verificar su proyecto). El
  mensaje nombra el namespace ajeno y el namespace permitido. La skill y
  las descripciones de las tools enuncian la regla: se lee entre
  proyectos, se escribe sólo en el propio. 37 pruebas de node en verde.
- L15 (T5): documentación actualizada en README (sección de permisos y
  lista de la CLI), `docs/projects.md` (protocolo del agente y los
  comandos de permiso), `docs/contracts/v1/README.md` (`allow_read_all`
  en el contrato) y `docs/PRD-Mnexis.md` §7 (la decisión de producto:
  lecturas abiertas entre proyectos, escrituras cerradas al proyecto).
- L16 (T3, delegado a `gentle-ai-worker`, revisado por el padre):
  `src/application/workspace.rs` resuelve el workspace con la misma clave
  byte a byte que la extensión (`git:<remote normalizado>` >
  `path:<repo root>` > `path:<cwd>`), y el namespace sale del binding de
  esa clave, si no del marker `.mnexis-namespace` más cercano, si no nada
  (la allowlist del profile sigue decidiendo). El guard vive en
  `src/cli/commands.rs`: `save`, `update`, `supersede`, `delete` y
  `restore` fallan con FORBIDDEN (exit 4) y un mensaje que nombra el
  namespace permitido y el `project bind` que lo cambia; para los
  comandos por UUID el blanco es el namespace de la memoria
  (`MemoryService::namespace_of`, sólo identidad: ni contenido ni
  autorización) y el supersede chequea los dos lados.
- L17 (T3, evidencia): RED del worker: tres tests de integración
  fallando con `left: Some(0) right: Some(4)` (el guard no existía) y
  12 pruebas unitarias en rojo con el módulo stub. GREEN: 15 unitarias
  del módulo, 57 en `cli_commands` (6 nuevas) y 346 en la suite completa.
  Diferencial contra una copia del árbol base (`git archive`): fase sin
  marker → 16 pasos con stdout/stderr/código de salida idénticos; fase
  con marker → sólo cambian las cinco escrituras que salen del proyecto
  (exit 4). `cargo fmt --check` limpio y clippy en 36 warnings (la línea
  base). El padre volvió a correr la suite completa: 346 en verde.
- L18 (T3, riesgo aceptado): en un workspace que resuelve, un UUID que
  vive en otro namespace responde FORBIDDEN (exit 4) en lugar del
  NOT_FOUND del profile: se confirma que esa memoria existe. Es el
  precio de S9 ("nombrar el namespace permitido"), sólo aplica cuando el
  guard dispara, y con las lecturas abiertas por defecto el agente puede
  leer esa memoria igual. Queda anotado para T6.
- L19 (coherencia, 5f5e119): la extensión resolvía el namespace a partir
  del nombre del directorio y la CLI a partir del marker: podían
  discrepar sobre qué proyecto era un directorio. La extensión lee ahora
  el marker con la misma regla (el más cercano gana, uno vacío sigue
  subiendo, el binding manda sobre el marker) y la documentación decía
  que el marker formaba parte de la clave: no es así, la clave identifica
  el workspace y el marker nombra su namespace. 41 pruebas de node.
- L20 (T3-bis, c1ab61f): el guard del CLI no cubría el servidor MCP, que
  es el mismo binario bajo otro subcomando y el camino que usa un agente
  cuando el host habla MCP. El servidor resuelve ahora el mismo
  workspace y rechaza con FORBIDDEN una escritura hacia otro namespace
  (`memory_save`, `memory_update`, `memory_supersede`, `memory_delete`,
  `memory_restore`), antes de que el servicio autorice. La prueba nueva
  (`the_server_refuses_a_write_into_another_namespace`) levanta el
  servidor con un marker de workspace y comprueba el rechazo y el
  namespace propio admitido.
- L21 (cierre del hueco, 7ca90a2): `mnexis import` era la última vía de
  escritura que podía salirse del workspace: escribe un archivo entero,
  un namespace por registro, y nada verificaba a qué namespace apuntaban
  esos registros. Ahora los namespaces del archivo se comprueban contra
  el workspace antes de importar, así que un archivo que pertenece a
  otro proyecto se rechaza en vez de escribirse. `purge` queda fuera a
  propósito: es una operación administrativa sobre todo el profile, no
  sobre un namespace.
- L22 (review nativa, declinada): con RDD habilitado, `inspect` sobre el
  candidato (sha256:5f3fd90d…, 30 archivos, 2444 líneas) ofreció
  `review.start`; el consentimiento lo resolvió el host y quedó
  **declinado** para este candidato (`consent-declined-this-candidate`).
  No se creó lineage y no hubo mutaciones. Una declinación es por
  candidato: no apaga la review para otros candidatos ni autoriza
  entrega. La verificación funcional de T6 (verificador independiente)
  sigue siendo la prueba que acompaña a este cambio.

