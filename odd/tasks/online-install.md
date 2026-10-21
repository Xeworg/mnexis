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
| T5 | S4, S8, S9 | `mnexis update [--check]`: servicio de actualización (HEAD del redirect, caché diaria, descarga + verificación + reemplazo atómico) | |
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
  lleva la versión porque la release ya la.namespacía; el checksum de cada
  artefacto vive en `SHA256SUMS`, que el workflow de T2 fusiona en un solo
  archivo. El script **se niega a empaquetar** un binario cuya versión no
  coincide con la pedida (empaquetar 0.2.0 con un binario 0.1.0 es el
  error que nadie ve hasta que un usuario reporta otra versión). 23
  aserciones en `tests/installer/11_release_package_test.sh` en verde
  (contenido del archivo, `sha256sum -c`, idempotencia del checksum,
  segunda plataforma, y los cuatro rechazos). `shellcheck` no está
  instalado en esta máquina: la verificación del shell es la prueba de
  comportamiento, no un lint.
