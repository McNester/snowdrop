# Краткие notes для предзащиты

## Слайд 1

Snowdrop — это не просто дипломная база данных, а задел под коммерческий продукт. Идея в том, чтобы ускорять SQL-нагрузку и при этом не ломать экосистему PostgreSQL.

## Слайд 2

Проблема рынка простая: либо мы остаёмся на PostgreSQL и упираемся в производительность аналитических read-path сценариев, либо уходим в отдельные специализированные системы и усложняем архитектуру. Snowdrop — это попытка взять лучшее из обоих подходов.

## Слайд 3

Snowdrop принимает PostgreSQL wire protocol, поэтому приложение продолжает подключаться как к PostgreSQL. Внутри обязанности разделены между orchestrator, Neo4j metadata layer, RocksDB storage, columnar cache и WAL.

## Слайд 4

Главная идея архитектуры — не заставлять один движок одинаково хорошо делать всё. Schema metadata живёт отдельно, durable row storage и secondary indexes отдельно, а аналитические сканы ускоряются отдельным columnar cache path.

## Слайд 5

Read path показывает, откуда берётся ускорение: если запрос scan-heavy, данные читаются через in-memory columnar arrays и SIMD filtering. Если нужен indexed lookup, используется RocksDB secondary-index path. Более сложные запросы идут через relation pipeline.

## Слайд 6

Важная часть ценности продукта — миграция. Сначала preflight оценивает базу PostgreSQL, потом строится безопасный translation preview, потом идёт data load и verification. Это снижает риск перехода.

## Слайд 7

На TPC-H уже есть хороший сигнал: на Q6 Snowdrop быстрее PostgreSQL. На JOIN-heavy сценариях он пока слабее, и это важно честно говорить. Но продукт уже показывает, где его сильная сторона — read-heavy analytical/operational scans.

## Слайд 8

Лучший кандидат на миграцию — это не любая PostgreSQL база, а типичный backend с большой read-нагрузкой: ticketing, orders, payments, subscriptions, CRM-like operational database.

## Слайд 9

Здесь стоит подчеркнуть, что проект уже не на уровне идеи: есть schema fidelity, typed semantics, migration workflow и автоматическая миграция для safe subset.

## Слайд 10

Финальная мысль: Snowdrop — это путь к ускорению SQL workload без полного переписывания продукта. То есть не “выкиньте PostgreSQL-мир”, а “сохраните совместимость и ускорьте то, что сейчас реально тормозит бизнес”.
