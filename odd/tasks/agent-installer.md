# agent-installer — Instalación de la integración con agentes desde el binario y la TUI

El binario de mnexis pasa a ser el único que instala la integración con los
agentes (skill y extensión), con los assets embebidos en el ejecutable. La
TUI abre con un menú donde esa instalación se gestiona sin comandos.

## Specs

- S1: "veo necesario hacer que en la tui existan las opciones de instlacion
  para los agentes pi y opencode que son los que tenemos" — la TUI ofrece la
  instalación para los dos entornos soportados: `pi` y `opencode`.
- S2: "tambien que se muestren al instalar que sea el programa quien lo
  gestione" — al instalar se muestra qué se instala (asset, destino,
  archivos), y es el programa (el binario) quien gestiona la instalación.
- S3: "ejemplo lo instalo y me abre el menu de instalacion de agentes" — al
  abrir `mnexis tui` aparece un menú con las opciones (instalar agentes,
  ver memorias, …).
- S4: "haz un menu al abrir vemos las opciones ejemplo instalar agentes y ver
  memoria es un ejemplo hazlo mas agrable para el usuario" — el menú es
  navegable con teclado, ordenado, con descripciones y una salida clara a
  la vista de memorias.
- S5: "El binario manda (una sola vía)" — `install.sh` deja de copiar los
  assets y delega en el binario; no hay dos implementaciones de instalación.
- S6: la instalación es idempotente y no destructiva: reinstalar sobrescribe
  sólo lo que mnexis distribuye y nunca toca skills o extensiones ajenas.
- S7: los assets embebidos son exactamente los del repo (`skills/` y
  `extensions/`), compilados con el binario: no pueden divergir.
- S8: `pi` recibe la skill **y** la extensión; `opencode` recibe la skill
  (no hay registro de extensiones para opencode).
- S9: la CLI expone la misma operación fuera de la TUI
  (`mnexis agent install|uninstall|status`), con salida legible y `--json`.
- S10: la TUI no inventa reglas: la instalación la resuelve la capa de
  aplicación, igual que el resto de la TUI no escribe SQL ni validaciones.

## Tasks

| ID | Spec | Route | Commit |
|----|------|-------|--------|
| T1 | S7, S8 | Assets embebidos (`src/assets.rs`): skill y extensión, sin `test/` | |
| T2 | S6, S8, S10 | Servicio de aplicación `agents`: target por env, install/uninstall/status idempotentes | |
| T3 | S9 | CLI `mnexis agent install|uninstall|status [--env]`, con lo que se instala y `--json` | |
| T4 | S1, S3, S4, S10 | TUI: menú inicial + vista de agentes (instalar/desinstalar/estado) | |
| T5 | S5 | `install.sh` delega en `mnexis agent install` | |
| T6 | S6 | Tests: unitarios del servicio, CLI, TUI (menú) y suite del instalador | |
| T7 | S1–S10 | Docs: `docs/tui.md`, `docs/install.md`, README | |

## Log

- L1 (user, verbatim): "veo necesario hacer que en la tui existan las opciones
  de instlacion para los agentes pi y opencode que son los que tenemos"
- L2 (user, verbatim): "tambien que se muestren al instalar que sea el
  programa quien lo gestione"
- L3 (user, verbatim): "ejemplo lo instalo y me abre el menu de instalacion
  de agentes"
- L4 (user, verbatim): "haz un menu al abrir vemos las opciones ejemplo
  instalar agentes y ver memoria es un ejemplo hazlo mas agrable para el
  usuario"
- L5 (decision): el binario es la única vía de instalación. `install.sh
  skills|extensions` delega en `mnexis agent install --env <env>`; el trade-off
  asumido es que la instalación vía shell deja de ser una copia pura sin
  toolchain y pasa a requerir el binario (que `install.sh` construye).
- L6 (evidence): la TUI actual tiene tres panes (Namespaces, Memories,
  Detail) y modos `Normal`/`Search`/`Form`/`Pick`/`Confirm`; el menú inicial
  se integra como un modo previo a `Normal` para no estorbar el layout de tres
  panes.
- L7 (evidence): el servicio de instalación resuelve los destinos por entorno
  (pi: `<agent-dir>/skills` y `<agent-dir>/extensions`, con
  `PI_CODING_AGENT_DIR`; opencode: `~/.config/opencode/skills`), instala de
  forma idempotente y sólo reemplaza los archivos que mnexis distribuye.
  Desinstalar borra los directorios vacíos de abajo hacia arriba y conserva
  cualquier archivo ajeno.
- L8 (evidence): `mnexis list --json` y `mnexis search --json` responden con
  un sobre `{count, items}`; el desinstalador del primer intento dejaba los
  directorios vacíos (`lib/`) porque emptiness se evaluaba antes de borrar.
- L9 (evidence): 302 pruebas de cargo en verde (18 binarios), 28 de la TUI
  (12 nuevas: navegación del menú, salida, vista de agentes, keymap) y 140
  asserts del instalador en verde.
- L10 (decision): la TUI abre en el menú con `App::new_at_menu`, y
  `App::new` sigue arrancando en `Mode::Normal` para no romper las pruebas
  de integración existentes de `tests/tui/`.
- L11 (decision): el menú y la vista de agentes están en inglés, como el
  resto de la TUI y como los artefactos técnicos del repo.
- L12 (note): `mnexis agent install` respeta `PI_CODING_AGENT_DIR`, así que
  desde el harness con esa variable exportada instala en el agent dir del
  harness; con `HOME` aislado y la variable ausente, instala bajo
  `~/.pi/agent`.
- L13 (review): la review nativa del candidato `sha256:d4a53a7b30117dde9b9220a133addf0ec8121e88d8e6ef91ba86ad0ef43af266`
  (34 archivos / 3897 líneas, nativeRisk "high") fue declinada por el host:
  `consent-declined-this-candidate`, `lineage_created: false`, sin mutación.
  El riesgo declarado (item 1: el binario escribe fuera del profile; item 3:
  cambia el contrato del instalador) se cubrió con verificación independiente,
  que encontró tres defectos y un riesgo nuevo:
  - el install escribía in situ: un `SKILL.md` symlinkado llevaba la
    escritura al archivo destino y lo destruía. Ahora se escribe a un nombre
    temporal y se renombra encima (reemplaza el enlace, no su destino).
  - `mnexis agent install --env opencode --only extension` salía 0 con
    "nothing to install"; ahora sale 2 y apunta a la skill.
  - código muerto tras la delegación (`shipped_skill_names`,
    `require_shipped_skills`, `shipped_extension_names`,
    `require_shipped_extensions`, `skills_dir_hint`) y el título del bloque
    de la vista de agentes en español.
  - el verificador sobreescribió por error el `SKILL.md` del agent dir del
    harness (el `PI_CODING_AGENT_DIR` del entorno); el archivo quedó
    byte-idéntico al del repo, sin daño. Su scratch en /tmp (182 MB) fue
    borrado.
- L14 (evidence): 304 pruebas de cargo en verde (2 nuevas de fidelidad de
  assets), 197 asserts del instalador (10 archivos; 57 nuevos en
  `10_agent_cli_test.sh`) y `cargo fmt --check` limpio.
