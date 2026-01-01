# Interceptor

HTTP mock server written in Rust.

## Usage

```bash
# Use configs from current directory
interceptor

# Specify config directory
interceptor -c ./configs

# Verbose logging
interceptor -v
```

## Configuration

Create JSON files with route definitions:

```json
{
  "name": "my-api",
  "port": 8080,
  "routes": [
    {
      "method": "GET",
      "route": "/api/users",
      "response": "{\"users\": []}",
      "status": 200,
      "headers": {
        "Content-Type": "application/json"
      }
    }
  ]
}
```

Ignore files using `.interceptorignore` (gitignore syntax).

## Build

```bash
cargo build --release
```
