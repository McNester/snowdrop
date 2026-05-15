# Руководство по тестированию Snowdrop

## Обзор

Теперь в Snowdrop есть отдельный фундамент для тестирования, который состоит из:

- фиксированных SQL-датасетов в `tests/datasets/`
- описаний suite-ов в `tests/golden/`, `tests/differential/`, `tests/sqllogictest/`, `tests/tap/`, `tests/postgres_style/`
- отдельного Docker-ориентированного раннера `snowdrop-testkit`
- одного общего генерируемого отчёта `reports/test-report.md`

Первая версия этой системы специально сделана Docker-first. Сначала подними сервисы через `docker compose up -d`, затем запускай команды harness-а из корня репозитория.

Если suite требует доступ к PostgreSQL, раннер сначала берёт пароль из `PGPASSWORD`, а затем из `POSTGRES_PASSWORD`.

## Структура папок

```text
tests/
  datasets/
    <dataset>/
      manifest.json
      schema.sql
      seed.sql
  golden/
  differential/
  sqllogictest/
  tap/
  postgres_style/

reports/
  test-report.md
  raw/
    snowdrop-testkit-results.json
```

## Формат датасетов

Каждый датасет лежит в `tests/datasets/<dataset>/` и содержит:

- `manifest.json`: metadata датасета, имя базы, порядок очистки таблиц, поддерживаемые suite-ы
- `schema.sql`: DDL схемы
- `seed.sql`: фиксированные INSERT-ы

Правила:

- использовать читаемые и ревьюемые SQL-фикстуры
- в v1 держать датасеты небольшими
- внутри fixture SQL предпочитать не fully-qualified таблицы
- считать эти файлы общим source of truth для Snowdrop и PostgreSQL в differential tests

## Поддерживаемые suite-ы

### Golden

- Запускает заранее описанный SQL на Snowdrop и фиксированном датасете
- Сравнивает точные строки результата или ожидаемую ошибку
- Это основной correctness suite

### Differential

- Запускает одинаковый SQL на Snowdrop и PostgreSQL
- Сравнивает нормализованные результаты
- Помечает неподдержанное поведение Snowdrop как `skip`, если ошибка явно говорит про unsupported или unimplemented поведение

### SQLLogicTest

- Использует ограниченное подмножество `.test`-файлов
- Поддерживаются:
  - `statement ok`
  - `statement error`
  - `query ... ----`
- В каждом файле обязателен заголовок `# dataset: <name>`

### TAP-style

- Покрывает readiness и интеграционные smoke-проверки
- Использует JSON-описания suite-ов в `tests/tap/`
- Пишет TAP-подобные файлы в `reports/tap/`

### PostgreSQL-style subset

- Использует пары `sql/` и `expected/` в `tests/postgres_style/`
- Повторяет организацию regression tests в стиле PostgreSQL
- Не пытается запускать literal upstream PostgreSQL tests

## Команды

Канонический workflow:

```powershell
docker compose up -d --build orchestrator
$env:PGPASSWORD = "<ваш-пароль-postgres>"
cargo run -p snowdrop-testkit -- list-datasets
cargo run -p snowdrop-testkit -- run-all --with-tpch
```

Другие команды:

```powershell
cargo run -p snowdrop-testkit -- load --dataset tiny_smoke
cargo run -p snowdrop-testkit -- run --dataset tiny_smoke --suite golden
cargo run -p snowdrop-testkit -- run --dataset edge_cases --suite differential
cargo run -p snowdrop-testkit -- run --suite sqllogictest
cargo run -p snowdrop-testkit -- run --suite tap
cargo run -p snowdrop-testkit -- run --suite postgres-style
cargo run -p snowdrop-testkit -- report
```

Примечания:

- `golden`, `sqllogictest` и `tap` можно запускать без PostgreSQL credentials.
- `differential`, `postgres-style` и `run-all --with-tpch` требуют, чтобы перед запуском был выставлен `PGPASSWORD` или `POSTGRES_PASSWORD`.
- `run` и `run-all` теперь делают небольшой Phase 3 runtime preflight. Если harness пишет, что Snowdrop устарел, пересобери `orchestrator` и запусти тесты снова. Обходить эту защиту стоит только осознанно через `SNOWDROP_SKIP_RUNTIME_PROBE=1`.

## Генерируемые артефакты

Основной human-facing артефакт:

- `reports/test-report.md`

В нём есть:

- metadata окружения
- inventory датасетов
- короткую Phase 3 summary со статусами supported / partial / explicitly unsupported
- summary по suite-ам и датасетам
- таблицы кейсов для golden, differential, sqllogictest, TAP-style и PostgreSQL-style suite-ов
- skipped или unsupported кейсы
- TPC-H benchmark summary, если использовался `--with-tpch`

Raw machine-readable артефакт лежит здесь:

- `reports/raw/snowdrop-testkit-results.json`

## Значения статусов

- `pass`: поведение совпало с ожиданием
- `fail`: запрос или workflow неожиданно упал
- `mismatch`: запрос выполнился, но вернул не тот результат
- `skip`: кейс не считается ошибкой, обычно потому что Snowdrop это поведение пока не поддерживает

## Связанные документы

- [benchmarking.md](/e:/snowdrop-1/docs/benchmarking.md:1)
- [sql-compatibility-matrix.md](/e:/snowdrop-1/docs/sql-compatibility-matrix.md:1)
- [postgres-migration-readiness-roadmap.md](/e:/snowdrop-1/docs/postgres-migration-readiness-roadmap.md:1)
- [reports/README.md](/e:/snowdrop-1/reports/README.md:1)

## Ограничения

- В v1 поддерживается только Docker-based runtime
- Нет literal-совместимости с upstream PostgreSQL regression/TAP tests
- Поддержка `sqllogictest` намеренно частичная
- Большие production-like датасеты пока вне scope этой первой тестовой основы
