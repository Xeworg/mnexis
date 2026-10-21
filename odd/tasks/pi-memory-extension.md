# pi-memory-extension — Extensión de pi para el motor de memoria mnexis

Una extensión nativa de pi que expone el motor de memoria mnexis como tools
de pi, resuelve sola el binding workspace → namespace y aplica el gate de
permisos (autorización humana explícita **una vez por namespace**).

## Specs

- S1: "veo bien a opcion uno pero no veo necesario que el usuario tenga que
  confirmar cada vez que se deba guardar memoria el agente deberia hacerlo
  solo" — dentro de un namespace ya autorizado, el agente guarda sin
  preguntar. La confirmación humana ocurre **una sola vez**, al autorizar el
  namespace; después, los guardados son silenciosos.
- S2: "ahora trabaja la de pi creo que esta ocupa usar la api que tiene pi
  para hacer extensiones" — la implementación usa la API de extensiones de
  pi (`pi.registerTool`, `ctx.ui.confirm()`), no un script suelto ni un
  MCP server registrado a mano.
- S3: "la skill se usa unicamente cuando se le pide al agente puede ser
  tambien por falta de un harness ya que pi viene vacio complemetamente" —
  las tools de memoria están disponibles en **toda** sesión, sin que el
  usuario las pida ni sin depender de que el modelo descubra una skill.
- S4: "creo que esta ocupa usar la api que tiene pi para hacer extensiones"
  — la extensión se distribuye desde este repo y el instalador la coloca
  en el directorio de extensiones de pi.
- S5: la key del workspace es canónica y determinística:
  `git:<remote normalizado>` = protocolo fuera, sufijo `.git` fuera, **toda**
  la URL en minúsculas, `/` y `:` reemplazados por `-`, los puntos del host
  conservados (ej. `git:github.com-xeworg-aegispi`). Sin remote:
  `path:<raíz absoluta del repo>`.
- S6: el gate de permisos es del humano, no del agente: sin autorización
  explícita, la extensión **no** concede permiso de escritura. En modo sin
  UI (headless, `pi -p`) informa que la autorización está pendiente en vez
  de concedérsela sola.
- S7: la extensión nunca edita `mnexis.toml` ni `projects.toml` a mano: solo
  comandos del CLI (`mnexis init`, `mnexis project bind`,
  `mnexis config allow-namespace`, `mnexis save`, `mnexis search`,
  `mnexis list`, `mnexis show`, `mnexis supersede`).
- S8: los errores del CLI se mapean a mensajes accionables: el modelo ve el
  código de salida y el stderr real, no un "algo falló".

## Tasks

| ID | Spec | Route | Commit |
|----|------|-------|--------|
| T1 | S2, S3 | Extension skeleton: `pi.registerTool` con tools nativas de memoria, exposed al modelo en cada sesión | |
| T2 | S5 | Resolución workspace → namespace: key canónica, git/path, binding existente o namespace nuevo | |
| T3 | S1, S6, S7 | Gate de autorización: una vez por namespace, `ctx.ui.confirm()`, headless → pendiente | |
| T4 | S8 | Capa de invocación del CLI: spawn, parseo JSON, mapeo de exit codes | |
| T5 | S1–S8 | Tests: unitarios de key/resolución/gate + prueba en vivo en container con pi plain | |
| T6 | S4 | Instalador: `install.sh extensions --env pi` + docs (README, docs/install.md) | |

## Log

- L1 (user, verbatim): "usa aegispi nuestro proyecto en instalalo para hacer
  la prueba puedes instalarlo ahi"
- L2 (user, verbatim): "creo que la mejor opcion seria tener un entorno
  aislado podemos usar un contenedor de docker"
- L3 (user, verbatim): "como incio docker no recuerdo"
- L4 (user, verbatim): "o si tienes mayor control de podman puedes usarlo
  tambien"
- L5 (user, verbatim): "si la skill se usa unicamente cuando se le pide al
  agente puede ser tambien por falta de un harness ya que pi viene vacio
  complemetamente, igual intentemos reforzar y luego probaremos con opencode"
- L6 (user, verbatim): "perfecto, ahora trabaja la de pi creo que esta ocupa
  usar la api que tiene pi para hacer extensiones"
- L7 (decision): la extensión implementa el protocolo completo (auto-resolución
  de namespace + gate en código). La confirmación humana es **por namespace y
  única**, no por guardado: dentro de un namespace autorizado el agente
  guarda sin preguntar (decisión del usuario en la revisión de opciones).
- L8 (evidence, live test en podman con pi plain y opencode): la skill se
  anunciaba y se usaba, pero el modelo elegía rutas propias (AGENTS.md,
  `~/.config/opencode`) y la resolución del namespace quedaba en el modelo.
  De ahí la extensión: capacidad determinista, no instrucciones.
- L9 (evidence, live test): la key del workspace era ambigua (el doc decía
  "lowercase the host" y a la vez reemplazaba los puntos, contradiciendo su
  propio ejemplo); dos modelos produjeron keys distintas. S5 lo fija.
- L10 (review): la review nativa de este worktree fue declinada por el host
  para el candidato `sha256:b49ede66ef62605600cbe519f1f4cc56dd1a8173376d3a2ff5357c327c5b2da7`
  (10 archivos / 200 líneas, riesgo nativo "high" por señalización de proceso
  en `install.sh`): `consent-declined-this-candidate`, `lineage_created: false`,
  sin mutación. Decisión acotada al candidato, no un kill switch.
- L11 (context backstop): la implementación de T1–T4 se delegó a un worker
  acotado por superficies de edición; la verificación (T5) y el instalador
  (T6) se ejecutaron en el orquestador.
- L12 (decision): la extensión registra sus tools con `exposure: "direct"`,
  de modo que quedan declaradas al modelo en toda sesión. La resolución del
  namespace y el gate viven en la extensión; la skill queda como documentación
  y fallback para hosts sin extensión.
- L13 (defect found by live test): la firma real de `execute` en pi es
  `execute(toolCallId, params, signal, onUpdate, ctx)`, no `execute(args, ctx)`.
  Con la firma asumida, la extensión recibía el `toolCallId` como argumentos:
  `mnexis_save` guardaba `content: "undefined"` y las lecturas ignoraban los
  filtros. Verificado contra `dist/core/extensions/types.d.ts`.
- L14 (defect found by live test): `mnexis list --json` y `mnexis search --json`
  responden con un sobre `{ count, items }`, no con un array. La extensión
  ahora desempaqueta el sobre.
- L15 (defect found by live test): el gate de la extensión era evadible — el
  modelo ejecutaba `mnexis config allow-namespace` por bash y se auto-autorizaba.
  La extensión ahora intercepta `tool_call` y bloquea (a) los comandos bash que
  conceden permisos o hacen bind y (b) las escrituras/edit sobre
  `mnexis.toml` y `projects.toml`. Los comandos de la persona no se tocan.
- L16 (evidence, live test en podman con pi plain y deepseek-v4.1-flash):
  Fase 1 (headless, sin autorizar): la extensión resolvió el namespace, la
  escritura falló con el mensaje de autorización pendiente, el intento de
  auto-autorización por bash fue bloqueado y el agente pidió la autorización a
  la persona; el profile quedó con `write_namespaces = ["default"]` y cero
  memorias. Fase 2 (el operador concede `mnexis config allow-namespace
  aegispi`): Fase 3 (segunda sesión): el agente guardó en silencio —sin
  preguntar nada— con tags y topic-key, y verificó la recuperación con
  `mnexis_search`, `mnexis_list` y `mnexis_show`.
- L17 (evidence): 31 pruebas unitarias (`node --test`) en verde y 140 asserts
  de la suite del instalador en verde, incluido `09_extensions_test.sh`
  (RED verificado: 9 fallos sin el soporte de extensiones en `install.sh`).
- L18 (review): la review nativa fue declinada por el host para este
  candidato (`sha256:301cd355ee01a42cc5b17acb195df8557a2ecf0b451db4be2bcded55030f40c2`,
  22 archivos / 1625 líneas, nativeRisk "high" por señalización de proceso):
  `consent-declined-this-candidate`, `lineage_created: false`, sin mutación.
  Como el riesgo declarado es item 2 (permisos y guards), se ejecutó verificación
  independiente: 6/6 puntos OK, sin defectos; 31/31 pruebas unitarias y 140/140
  asserts del instalador en verde, sin mutación del repo. Puntos verificados:
  gate (allow silencioso / confirm una vez / headless nunca autoriza), guard
  anti-bypass (bloquea grants y edición del profile; no toca `user_bash` ni las
  tools propias), flags del CLI contra `src/cli/{mod,commands,output}.rs`, forma
  del sobre JSON, normalización de la key e instalador de extensiones.
