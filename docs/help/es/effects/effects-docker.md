# El panel Efectos

Un objeto puede llevar varios efectos dinámicos a la vez, aplicados uno tras
otro. El panel [[docker.effects]] enumera los efectos del objeto seleccionado
en el orden en que se aplican, de arriba abajo. Ábralo con
[[menu.window > menu.window.dockers > menu.window.dockers_effects]].

![El panel Efectos](../../img/effects-effects-docker.png)

## Gestionar los efectos {#stack}

Cada fila de la lista tiene:

- una casilla que activa o desactiva el efecto. Un efecto desactivado
  conserva su lugar y sus ajustes, pero no se dibuja, ni se exporta, ni se
  imprime;
- el nombre del efecto, atenuado mientras está desactivado;
- flechas arriba y abajo que adelantan o retrasan el efecto, lo que cambia el
  resultado (por ejemplo, una silueta de una envoltura, o una envoltura de una
  silueta);
- una cruz que quita el efecto.

Cada cambio es un paso de deshacer. Cada efecto se sigue editando con su
propia herramienta o panel.

## Redondear esquinas y Rebanar {#outline-effects}

Dos entradas de los efectos de capa, en lo alto del panel, rehacen el
contorno tal como se dibuja y el objeto queda como estaba (un texto sigue
editable):

- **[[effect.round_corners]]** redondea todas las esquinas de una curva, de
  las curvas de un grupo o de las letras de un texto con un radio. **Hasta**
  lo limita a las esquinas más agudas que un ángulo (180° las toma todas) y
  los botones **Esquinas** eligen todas, solo las exteriores o solo las
  interiores.
- **[[effect.slice]]** corta el objeto en tiras paralelas: cantidad, ángulo
  de los cortes y separación; **Desplazamiento** desliza cada tira a lo largo
  del corte, alterno o aleatorio; **Giro** gira cada tira un poco al azar;
  **Semilla** elige otra variación; **Redondeo** redondea las esquinas de las
  tiras.

[[menu.object.convert_to_curves]] (o Separar) convierte el resultado en
curvas normales.

## Otros comandos de efectos {#commands}

- [[menu.effects > menu.effects.flatten]] convierte los efectos de los objetos
  seleccionados en objetos normales.
- [[menu.effects.picker]] muestra una lista de efectos que aplicar a la
  selección.
- [[menu.object > menu.object.clear_effect]] quita el efecto de la selección.
- [[menu.object > menu.object.copy_effect]] copia un efecto de otro objeto, y
  [[menu.object.clone_effect]] crea un clon que lo sigue; vea
  [Copiar y clonar efectos](copy-and-clone.md).
