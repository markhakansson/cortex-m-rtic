# Типы, Send и Sync

Каждая функция в модуле `app` принимает структуру `Context` в качесте первого параметра.
Все поля этих структур имеют предсказуемые, неанонимные типы,
поэтому вы можете написать обычные функции, принимающие их как аргументы.

Справочник по API определяет как эти типы генерируются на основе входных данных.
Вы можете также сгенерировать документацию к вашему крейту программы (`cargo doc --bin <name>`);
в документации вы найдете структуры `Context` (например `init::Context` и
`idle::Context`).

Пример ниже показывает различные типы, сгенерированные атрибутом `app`.

``` rust
{{#include ../../../../examples/types.rs}}
```

## `Send`

[`Send`] - это маркерный трейт для "типов, которые можно передавать через границы
потоков", как это определено в `core`. В контексте RTIC трейт `Send` необходим
только там, где возможна передача значения между задачами, запускаемыми на
*разных* приоритетах. Это возникает в нескольких случаях: при передаче сообщений,
в разделяемых `static mut` ресурсах и при инициализации поздних ресурсов.

[`Send`]: https://doc.rust-lang.org/core/marker/trait.Send.html

Атрибут `app` проверит, что `Send` реализован, где необходимо, поэтому вам не
стоит волноваться об этом. В настоящий момент все передаваемые типы в RTIC должны быть `Send`, но
это ограничение возможно будет ослаблено в будущем.

## `Sync`

Аналогично, [`Sync`] - маркерный трейт для "типов, на которые можно безопасно разделять между потоками",
как это определено в `core`. В контексте RTIC типаж `Sync` необходим только там,
где возможно для двух или более задач, запускаемых на разных приоритетах получить разделяемую ссылку (`&-`)  на
ресурс. Это возникает только (`&-`) ресурсах с разделяемым доступом.

[`Sync`]: https://doc.rust-lang.org/core/marker/trait.Sync.html

Атрибут `app` проверит, что `Sync` реализован, где необходимо, но важно знать,
где ограничение `Sync` не требуется: в (`&-`) ресурсах с разделяемым доступом, за которые
соперничают задачи с *одинаковым* приоритетом.

В примере ниже показано, где можно использовать типы, не реализующие `Sync`.

``` rust
{{#include ../../../../examples/not-sync.rs}}
```
