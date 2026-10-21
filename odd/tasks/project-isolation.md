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
| T3 | S5, S9 | Aislamiento de escritura en el CLI: resolución del namespace del workspace actual (binding por clave, marker `.mnexis-namespace`) y guard de escritura en save/update/supersede/delete/restore | |
| T4 | S7, S8 | Extensión acotada al proyecto: supersede rechaza objetivos fuera del namespace del workspace; descripciones de las tools y protocolo de la skill | |
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

