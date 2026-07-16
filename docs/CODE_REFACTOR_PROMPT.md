PROMPT

## Objetivo

Analiza completamente el repositorio **jacodelia/tanu** y diseña e implementa una nueva arquitectura que permita ejecutar Tanu como un servicio centralizado (headless), accesible desde múltiples clientes remotos conectados mediante **Tailscale**, similar al modelo de conexión de aplicaciones como **Navidrome**, **Jellyfin**, o incluso **Spotify Connect** (a pequeña escala para un homelab).

El objetivo NO es únicamente agregar acceso remoto.

El objetivo es transformar Tanu desde una aplicación local hacia una plataforma distribuida, manteniendo la simplicidad del proyecto original.

---

# Contexto

Escenario objetivo:

```
                  Internet
                      │
                Tailscale Mesh
                      │
        ┌─────────────┴─────────────┐
        │                           │
   Laptop                      SteamDeck
        │                           │
        │                           │
        └─────────────┬─────────────┘
                      │
                 Raspberry Pi
              MiniPC / NAS / Server
              Ejecutando Tanu Server
                      │
                Biblioteca Musical
                SSD / HDD / NAS
```

Los clientes NO necesitan tener acceso directo a la biblioteca.

Toda la lógica vive en el servidor.

Los clientes son únicamente consumidores remotos.

---

# Objetivos funcionales

El nuevo sistema debe soportar:

* múltiples usuarios
* múltiples clientes conectados simultáneamente
* sesiones independientes
* navegación remota
* búsqueda
* cola de reproducción
* favoritos
* playlists
* metadata
* streaming de audio
* sincronización de estado
* control remoto

Idealmente cada usuario mantiene:

* historial
* biblioteca
* cola
* playlists
* preferencias

---

# Nueva arquitectura deseada

Separar completamente el proyecto en capas.

```
                 +----------------+
                 | CLI Cliente    |
                 +-------+--------+
                         |
                         |
                 HTTP/gRPC/WebSocket
                         |
                         |
            +------------v------------+
            |      API Server         |
            |   (Headless Tanu)       |
            +------------+------------+
                         |
           +-------------+-------------+
           |                           |
     Playback Engine            Library Engine
           |                           |
           +-------------+-------------+
                         |
                  Metadata Index
                         |
                  Audio Library
```

La interfaz de consola nunca debe acceder directamente al filesystem.

Siempre debe comunicarse con el servidor.

---

# Componentes

## 1. Headless Server

Crear un nuevo binario.

Ejemplo:

```
tanu-server
```

Debe ejecutar:

* API
* autenticación
* indexador
* reproductor
* administración de usuarios
* streaming
* sincronización

Debe ejecutarse como daemon.

Idealmente:

```
systemd

docker

docker compose

podman
```

---

## 2. Cliente

Crear un cliente liviano para terminal, web y otro para mobile (android, ios, raspberry pi).

```
tanu-cli
```

```
tanu-cli-web
```

```
tanu-cli-mobile
```

el cliente solo realiza:

* login
* búsqueda
* exploración
* reproducción
* control remoto

No indexa música.

No administra archivos.

No necesita acceso al NAS.

---

## 3. API

Diseñar una API limpia.

Separar:

```
Authentication

Users

Library

Albums

Artists

Tracks

Playlists

Queue

Playback

Search

Metadata

Streaming

Health

Metrics
```

---

## 4. Comunicación en tiempo real

Agregar WebSocket o gRPC streaming para eventos.

Ejemplos:

```
Now Playing

Queue Updated

Playback Changed

Volume

Pause

Resume

Client Connected

Client Disconnected
```

Evitar polling.

---

## 5. Streaming

Implementar streaming HTTP con soporte para:

```
Range Requests

Seeking

Buffering

Progressive Streaming
```

No cargar canciones completas en memoria.

Utilizar streams.

---

## 6. Autenticación

Primera versión:

```
usuario
password
```

Posteriormente soportar:

* OAuth2
* Tailscale Identity
* OIDC
* Auth Proxy

Nunca almacenar contraseñas en texto plano.

Utilizar Argon2 o bcrypt.

---

## 7. Persistencia

No almacenar información crítica en memoria.

Agregar base de datos.

Recomendación:

SQLite inicialmente.

Diseñar un DAL que permita migrar luego a:

* PostgreSQL
* MariaDB

Modelos:

```
Users

Tracks

Albums

Artists

Playlists

PlaylistTracks

Favorites

History

Sessions

Devices

Scans

Configuration
```

---

# Indexador

Separar el indexador del reproductor.

El indexador debe:

* detectar cambios
* escanear incrementalmente
* soportar rescans completos
* actualizar metadata

Idealmente mediante eventos del filesystem.

Linux:

```
inotify
```

macOS:

```
FSEvents
```

Windows:

```
ReadDirectoryChangesW
```

---

# Cache

Agregar una capa de cache.

Separar:

```
Metadata Cache

Album Art Cache

Search Cache

Thumbnail Cache

Waveform Cache
```

Evitar recalcular información repetidamente.

---

# Metadata

Agregar soporte para:

```
ID3v2

FLAC

Vorbis

MP4

APE
```

Extraer:

* artista
* álbum
* género
* año
* duración
* BPM
* portada
* track number
* disc number
* composer

---

# Escalabilidad

Diseñar pensando en miles de álbumes.

No asumir una biblioteca pequeña.

Evitar:

```
leer todo el filesystem

leer todas las canciones

reindexar completo

mantener todo en RAM
```

Implementar:

* paginación
* lazy loading
* consultas SQL
* índices

---

# Búsqueda

Crear un motor de búsqueda.

Debe soportar:

* artista
* álbum
* canción
* género

Idealmente:

SQLite FTS5.

Diseñar para poder migrar luego a:

* Meilisearch
* Typesense

---

# Playback

Separar completamente el motor.

Interfaces sugeridas:

```
Player

QueueManager

OutputDevice

Decoder

Resampler
```

Permitir múltiples implementaciones.

---

# Cola

Cada usuario posee:

```
Queue

History

Shuffle

Repeat

Current Position
```

No utilizar una cola global.

---

# Sesiones

Cada cliente mantiene una sesión.

Ejemplo:

```
Laptop

Desktop

SteamDeck

Phone
```

Sincronizar:

* reproducción
* volumen
* estado

---

# API Versioning

Diseñar:

```
/api/v1
```

Nunca exponer endpoints sin versionado.

---

# Configuración

Utilizar un archivo TOML o YAML.

Ejemplo:

```
server:
    port:

library:

database:

cache:

authentication:

streaming:

tailscale:
```

---

# Logging

Agregar logging estructurado.

Preferentemente:

```
JSON

levels

trace

debug

info

warn

error
```

---

# Observabilidad

Agregar:

```
/metrics

/health

/ready
```

Compatibles con Prometheus.

---

# Seguridad

Nunca asumir una red confiable.

Agregar:

* autenticación
* autorización
* rate limiting
* CORS configurable
* CSRF cuando aplique
* validación de entrada
* límites de tamaño
* timeouts

---

# Docker

Crear:

```
Dockerfile

docker-compose.yml
```

Persistencia mediante volúmenes.

---

# Systemd

Agregar unidad:

```
tanu.service
```

---

# Tailscale

La solución debe funcionar perfectamente sobre una red Tailscale.

No depender de NAT traversal manual.

No abrir puertos públicos.

Idealmente:

```
Servidor

http://100.x.x.x:PORT

o

http://tanu.tailnet.ts.net
```

Los clientes únicamente conocen la URL del servidor.

---

# Arquitectura interna sugerida

```
cmd/

internal/

pkg/

api/

player/

scanner/

metadata/

database/

repository/

services/

models/

auth/

cache/

config/

streaming/

websocket/

queue/

playlists/

search/

events/

metrics/
```

---

# Principios de diseño

Aplicar:

* Clean Architecture
* SOLID
* Dependency Injection
* Repository Pattern
* Service Layer
* Domain Driven Design (cuando aporte valor)
* Interfaces pequeñas
* Alta cohesión
* Bajo acoplamiento

---

# Consideraciones de rendimiento

Evitar:

* variables globales
* mutex innecesarios
* copias grandes en memoria
* bloqueos largos
* lecturas repetidas

Preferir:

* context.Context
* goroutines controladas
* worker pools
* canales
* cancelación
* streaming

---

# Compatibilidad futura

Diseñar para soportar posteriormente:

* interfaz web
* cliente TUI
* cliente móvil
* API pública
* reproducción distribuida
* múltiples bibliotecas
* múltiples servidores
* federación
* plugins
* transcodificación
* Chromecast
* AirPlay
* DLNA
* Sonos
* Spotify Connect-like
* sincronización entre dispositivos

---

# Roadmap recomendado

## Fase 1

* Refactor del código existente
* Separación de responsabilidades
* Headless Server
* Cliente remoto
* API REST

## Fase 2

* Usuarios
* SQLite
* Playlists
* Favoritos
* Historial

## Fase 3

* Streaming
* WebSockets
* Cola remota
* Sincronización

## Fase 4

* Docker
* systemd
* Observabilidad
* Configuración

## Fase 5

* OAuth
* OIDC
* Plugins
* gRPC
* Clusterización ligera

---

# Entregables esperados

1. Análisis detallado de la arquitectura actual del repositorio.
2. Identificación de acoplamientos y deuda técnica.
3. Propuesta de arquitectura objetivo con diagramas (Mermaid).
4. Plan de migración incremental que minimice regresiones.
5. Refactor del código respetando compatibilidad cuando sea posible.
6. Implementación del modo `tanu-server`.
7. Implementación del cliente remoto `tanu`.
8. API REST documentada (OpenAPI/Swagger).
9. Comunicación en tiempo real mediante WebSockets o gRPC Streaming.
10. Persistencia con SQLite y capa de acceso desacoplada.
11. Sistema de autenticación y gestión de sesiones.
12. Streaming eficiente con soporte para HTTP Range Requests.
13. Dockerfile, Docker Compose y unidad systemd.
14. Suite de pruebas unitarias, de integración y benchmarks para componentes críticos.
15. Documentación técnica completa de la nueva arquitectura, decisiones de diseño (ADR) y guía de despliegue para un homelab con Tailscale.

---

# Criterios de éxito

La implementación se considerará exitosa cuando:

* Un único servidor Tanu pueda centralizar una biblioteca musical.
* Múltiples clientes puedan conectarse simultáneamente mediante Tailscale.
* Cada usuario disponga de sesiones, colas y preferencias independientes.
* La arquitectura permita evolucionar hacia clientes web, móviles y TUI sin cambios profundos en el núcleo del sistema.
* El diseño sea mantenible, extensible y preparado para futuras capacidades como plugins, transcodificación y reproducción distribuida.
* Las responsabilidades estén claramente desacopladas y alineadas con principios de Clean Architecture.
