# online-install — que el usuario instale mnexis desde un enlace y sepa actualizarlo

El usuario instala mnexis con un `curl | sh` servido por GitHub Releases,
que baja un binario precompilado verificado por checksum; y la TUI le avisa
cuando hay una versión nueva y ofrece instalarla.

## Specs

- S1: "quiero la opcion 3 pero tambien quiero que al iniciar la tui muestre
  si hay actualizacion y tenga la opcion de actualizarse" — al abrir la TUI,
  si hay una versión nueva, el menú lo muestra y ofrece actualizarse.
- S2: "recomiendame algo se puede desde github? sin pagar un dominio, que
  sea mas confiable para el usuario" — la distribución y el instalador viven
  en GitHub Releases, sin dominio propio ni infraestructura de terceros; la
  confianza se apoya en el checksum publicado en la misma release y en un
  camino de fuente auditable.
- S3: el instalador se sirve desde
  `https://github.com/Xeworg/mnexis/releases/latest/download/install.sh` y
  admite fijar la versión (`MNEXIS_VERSION=v0.1.0`).
- S4: "Delegar en curl del sistema" — el binario y el script usan `curl` y
  `sha256sum` del sistema: ninguna dependencia nueva, ningún TLS propio, y
  si `curl` no está, el chequeo de actualización simplemente no se hace.
- S5: "Linux x86_64 y aarch64 (glibc)" — la primera release publica binarios
  para `linux-x86_64` y `linux-aarch64`; cualquier otra plataforma cae al
  camino de compilar desde fuente.
- S6: checksum ahora, firma después: la release publica `SHA256SUMS` y el
  README de la release dice explícitamente que todavía no está firmada; la
  firma (con clave pública en el repo) es una feature aparte.
- S7: una sola implementación — el script instala el binario y nada más
  (las skills y las extensiones las instala `mnexis agent install`); la
  actualización la implementa el binario, no el script.
- S8: el chequeo de actualización no usa la API de GitHub (60 req/h por IP):
  usa el redirect `HEAD` de `releases/latest`, que devuelve el tag en la
  cabecera `Location`. El resultado se cachea (una vez por día) para no
  pegarle a la red en cada arranque de la TUI.
- S9: reemplazar el binario en ejecución es atómico (temporal único en el
  mismo directorio + `rename`), igual que hace el instalador de assets; en
  Linux el proceso sigue con su inode viejo, así que la TUI no se rompe.
- S10: instalar es idempotente, no pide root, no escribe fuera de `$HOME`, y
  deja un camino de desinstalación.

## Tasks

| ID | Spec | Route | Commit |
|----|------|-------|--------|
| T1 | S2, S5, S6 | `scripts/package-release.sh`: build, empaquetado (`mnexis-linux-<arch>.tar.gz` con binario + LICENSE + README), `SHA256SUMS`; prueba bash que verifica el contenido y los checksums | |
| T2 | S3, S5 | `.github/workflows/release.yml`: tag `v*` → suite completa → artefactos → Release con `SHA256SUMS` e `install.sh`; `docs/release.md` con el procedimiento de corte | |
| T3 | S2, S3, S4, S5, S7 | `install.sh` servible por URL: detección de plataforma, descarga del artefacto, verificación del checksum, instalación atómica en `~/.local/bin`, pin de versión, fallback a compilar desde fuente | |
| T4 | S3, S10 | El one-liner cierra lanzando la TUI; idempotencia, sin root, camino de desinstalación | |
| T5 | S4, S8, S9 | `mnexis upgrade [--check]`: servicio de actualización (HEAD del redirect, caché diaria, descarga + verificación + reemplazo atómico) | |
| T6 | S1, S8 | La TUI muestra en el menú la versión nueva y ofrece actualizarse | |

## Log

- L1: "ahora me gustaria hacer un instalador online que ya habiamos hablado
  de ello anteriormente que el usuario lo pueda instalar desde un enlace
  hablemos de eso".
- L2 (contexto): `docs/install.md:3-6` declara hoy como no-objetivo
  explícito "no GitHub Releases / `curl | sh`"; `install.sh` compila desde
  fuente (exige Rust 1.95 y toolchain C) y no hay CI, ni releases, ni
  artefactos publicados (`publish = false` en Cargo.toml).
- L3 (decisiones del usuario): URL en GitHub Releases (`latest/download`),
  con pin por versión; distribución por binario precompilado con checksum y
  fallback a fuente; HTTP por `curl` del sistema (sin dependencias nuevas);
  plataformas linux x86_64 y aarch64 con glibc; la firma queda como
  feature posterior ("que me recomiendas" → checksum ahora, firma después).
- L4 (diseño): el chequeo de versión no usa la API de GitHub sino el
  redirect `HEAD` de `releases/latest` (la cabecera `Location` trae el
  tag): sin rate limit, sin token. La caché va en el data dir del profile.
- L5 (riesgo conocido): el checksum publicado en la misma release protege
  contra corrupción o descarga truncada, **no** contra una release o un CI
  comprometidos; esa capa es la firma, que queda como feature aparte.
- L6 (pendiente de verificación): el `install.sh` servido por URL tiene que
  seguir funcionando como el script del repo (una sola vía): detecta si hay
  `Cargo.toml` y compila, si no descarga.
- L7 (T1, evidencia): `scripts/package-release.sh` empaqueta una
  plataforma: `mnexis-linux-<arch>.tar.gz` con el binario, `LICENSE`,
  `README.md` y un `VERSION` (el instalador lo lee para rechazar un
  artefacto que no sea de la versión pedida). El nombre del artefacto no
  lleva la versión porque la release ya la namespacía; el checksum de cada
  artefacto vive en `SHA256SUMS`, que el workflow de T2 fusiona en un solo
  archivo. El script **se niega a empaquetar** un binario cuya versión no
  coincide con la pedida (empaquetar 0.2.0 con un binario 0.1.0 es el
  error que nadie ve hasta que un usuario reporta otra versión). 23
  aserciones en `tests/installer/11_release_package_test.sh` en verde
  (contenido del archivo, `sha256sum -c`, idempotencia del checksum,
  segunda plataforma, y los cuatro rechazos). `shellcheck` no está
  instalado en esta máquina: la verificación del shell es la prueba de
  comportamiento, no un lint.
- L8 (T2, evidencia): `.github/workflows/release.yml` corta una release
  con un tag `v*`: el job `test` corre fmt, clippy con `-D warnings`, la
  suite cargo y la suite del instalador; `build` compila y empaqueta en
  dos plataformas (`ubuntu-24.04` y `ubuntu-24.04-arm`); `publish` fusiona
  los `SHA256SUMS`, agrega el `install.sh` con su checksum, verifica los
  checksums y publica la release con `gh release create`. Las notas de la
  release dicen explícitamente que los artefactos **no están firmados**
  todavía (S6). Verificado localmente: el YAML parsea (ruby), los nueve
  pasos de shell pasan `bash -n`, y el terminador del heredoc queda en
  columna 0. **No verificable localmente**: la corrida real de Actions
  (requiere un tag y la red); la primera release la corta el usuario.
- L9 (T2, decisión de diseño): el workflow usa `gh release create` (la
  CLI de GitHub, ya presente) y no una action de terceros: menos
  superficie de suministro en el camino que publica binarios. El cache de
  cargo (`Swatinem/rust-cache`) es la única action de terceros, y sólo
  cachea.
- L10 (T3, diseño): **un solo script, dos modos**. El modo lo decide el
  propio script (si es un archivo) y no el directorio de trabajo: un
  script pasado por pipe no tiene archivo propio, así que siempre es el
  modo "standalone". Con `Cargo.toml` al lado compila (el camino de
  siempre); sin él, descarga. El script es bash, así que el one-liner es
  `curl … | bash`, no `| sh`: el `shebang` no se respeta cuando el
  script va por pipe.
- L11 (T3, seguridad): el instalador **no instala nada** si el checksum
  no coincide, si la release no publica checksum para el artefacto, o si
  el `VERSION` del artefacto no es el que se pidió. La instalación es
  atómica (temporal único en el mismo directorio + `mv`), así que el
  binario instalado nunca queda a medias. No pide root y no escribe fuera
  de `$HOME`.
- L12 (T3, evidencia): 27 aserciones nuevas en
  `tests/installer/12_online_install_test.sh` en verde, con la red
  falseada (un `curl` en el PATH sirve los archivos locales): instalación
  correcta, pin de versión (`/download/v0.1.0/`), latest por defecto,
  artefacto corrupto rechazado, checksum ausente rechazado, checksum sin
  línea para el artefacto rechazado, artefacto de otra versión rechazado,
  plataforma no soportada rechazada, idempotencia y ningún temporal
  dejando. La suite completa del instalador queda en **258
  aserciones en verde** (12 archivos), con el modo repo intacto. El
  fallback "clonar y compilar" no tiene prueba: necesita git y red.
- L13 (T4, diseño): el one-liner cierra con la TUI, que es el menú que
  muestra qué hacer después (memorias, integración de agentes). La TUI
  necesita una terminal en stdin y un script por pipe no la tiene: lee de
  `/dev/tty`. **Sólo arranca si la salida es una terminal**: en un script
  o en CI nadie está mirando, y colgar la instalación ahí sería un
  desastre. Antes abre el profile (`mnexis init`, idempotente) porque la
  TUI necesita la base de datos para navegar. `MNEXIS_NO_TUI=1` lo
  desactiva.
- L15 (T5, nombre): el comando se llama **`upgrade`** y no `update`:
  `mnexis update` ya actualiza una memoria en su lugar. La comprobación
  es `mnexis upgrade --check` y la actualización `mnexis upgrade`.
- L16 (T5, diseño): el servicio vive en `src/application/update.rs` y
  habla con el exterior a través de un trait `System`, así que las
  pruebas no tocan la red. No hay cliente HTTP en el binario: el servicio
  usa `curl` (HEAD para la versión, descarga para el artefacto),
  `sha256sum` (el checksum) y `tar` (el desempaquetado), los mismos
  comandos del instalador. La versión se lee del `Location` del redirect
  de `releases/latest` y **no** de la API de GitHub (60 req/h por IP).
- L17 (T5, evidencia): 11 pruebas unitarias del módulo (parseo del
  `Location`, comparación de versiones con pre-release, caché fresca y
  vencida, fallo de red que no es error) y 4 de integración en
  `tests/update_cli.rs` (con un `curl` falso en el PATH): la comprobación
  informa la versión nueva, la caché evita la segunda llamada, `--force`
  vuelve a preguntar, un fallo de red se reporta como "sin actualización"
  y el comando funciona sin profile (lo que actualiza es el binario, no
  las memorias). Suite completa: 20 suites en verde; `cargo fmt` limpio y
  clippy de vuelta en 36 warnings (los 5 nuevos se corrigieron).
- L14 (T4, evidencia): 31 aserciones en
  `tests/installer/12_online_install_test.sh` (4 nuevas): sin terminal la
  TUI no arranca; con terminal (pty de util-linux `script`) la
  instalación crea el profile y abre la TUI, verificado con un binario
  falso que registra cómo fue invocado. Suite del instalador completa:
  **262 aserciones en verde** (12 archivos).
