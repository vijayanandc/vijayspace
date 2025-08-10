# vijayspace

Prototype meta‑driven CRUD platform built with Spring Boot.

## Build

```bash
mvn clean package -Pdev
```

The `dev` profile uses an in‑memory H2 database with Flyway migrations. A sample
`contacts` module with a single `name` field is loaded from `data.sql`.

## Run

```bash
mvn spring-boot:run -Pdev
```

### Example

Create a record in the seeded `contacts` module:

```bash
curl -X POST http://localhost:8080/api/v1/data/contacts \
  -H 'Content-Type: application/json' \
  -d '{"name":"Alice"}'
```

List module metadata:

```bash
curl http://localhost:8080/api/v1/meta/modules
```
