# Crossy Road Clone

Juego 3D estilo Crossy Road desarrollado en Rust con WebGL.

## Requisitos

- Rust (rustup)
- wasm-pack

## Compilar

```bash
wasm-pack build --target web
```

## Ejecutar

Iniciar un servidor web local:

```bash
python3 -m http.server 8080
```

Abrir http://localhost:8080 en el navegador.

## Controles

- **W / ↑** - Avanzar
- **S / ↓** - Retroceder  
- **A / ←** - Izquierda
- **D / →** - Derecha
- **R** - Reiniciar

## Mecánicas

- Cruza carreteras evitando los coches (cubos rojos)
- Salta sobre los troncos (cubos marrones) para cruzar el agua
- Evita los árboles en el césped
- El puntaje aumenta al avanzar
