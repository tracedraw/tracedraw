# Símbolos

> *Tradução automática: esta página pode estar atrasada em relação à versão em inglês.*

Um **símbolo** é um objeto que você define uma vez e coloca muitas vezes. Cada
cópia, chamada **instância**, é um vínculo com a definição: edite a definição e
todas as instâncias mudam. Os símbolos deixam os arquivos pequenos e as partes
repetidas iguais. Eles ficam listados no painel [[docker.symbols]]
(<kbd>Ctrl+F3</kbd>).

![O painel Símbolos](../../img/objects-symbols.png)

## Criar um símbolo {#create}

1. Selecione os objetos que formam o símbolo.
2. Clique em [[docker.new_symbol]] no painel [[docker.symbols]], ou escolha
   [[menu.object > menu.object.symbol > menu.object.symbol_new]] ou
   [[menu.object > menu.object.create > menu.object.create_symbol]].

A seleção passa a ser a primeira instância do novo símbolo.

## Colocar instâncias {#insert}

O painel lista os símbolos do desenho, cada um com uma pequena imagem dos seus
objetos e o número de instâncias.
Clique em [[docker.insert]] ao lado de um símbolo para colocar uma nova
instância, ou escolha
[[menu.object > menu.object.symbol > menu.object.symbol_insert]]. As instâncias
podem ser movidas, escalonadas e giradas como qualquer objeto.

Uma instância copiada em um desenho e colada em outro leva o símbolo junto (e
os símbolos que ela mostra), então aparece com os mesmos objetos lá. Colar de
novo reaproveita o símbolo já trazido.

## Editar um símbolo {#edit}

Clique em [[docker.edit_symbol]] ao lado de um símbolo no painel, escolha
[[menu.object.symbol_edit]] no menu de contexto de uma instância ou
[[menu.object > menu.object.symbol > menu.object.symbol_edit]], ou clique duas
vezes em uma instância com a ferramenta Seleção. Os objetos do símbolo aparecem
na página onde essa instância os mostra e o resto da página fica esmaecido:
mova, remodele, mude as cores, adicione ou exclua objetos como de costume.
Clique em [[clip.finish]] na barra no canto superior esquerdo da janela de
desenho, escolha
[[menu.object > menu.object.symbol > menu.object.symbol_finish]] ou pressione
<kbd>Esc</kbd> sem nada selecionado: todas as instâncias mostram a mudança.
Desfazer percorre a edição passo a passo como qualquer outra. Clique em
[[docker.cancel_symbol_edit]] ao lado de [[clip.finish]] no painel para
descartar as mudanças e deixar o símbolo como estava.

Instâncias selecionadas, e grupos que as contêm, são contornados pelos objetos
que mostram, não por um quadrado.

Um símbolo não pode conter uma instância de si mesmo; Concluir avisa e a edição
continua até essa instância ser removida.

## Copiar um símbolo {#duplicate}

Clique em [[docker.duplicate_symbol]] ao lado de um símbolo no painel para
criar um novo símbolo com os mesmos objetos, com o nome do primeiro. Edite a
cópia sem mudar as instâncias do primeiro.

## Encontrar as iguais {#find-same}

Selecione um grupo de objetos e clique em [[docker.find_same]] no painel. Os
objetos do grupo que não são texto viram um símbolo (ou se juntam ao símbolo
que já desenha o mesmo), e todo grupo cujos objetos copiam o selecionado vira
uma instância dele: uma cópia em outra camada, e uma cópia dentro de outro
grupo, são encontradas do mesmo jeito. O texto de um grupo continua texto,
agrupado com a nova instância. A linha de status informa quantos grupos foram
substituídos e quantos foram ignorados; um grupo recolorido ou remodelado é
ignorado, não alterado.

## Substituir um símbolo por outro {#replace}

Clique em [[docker.replace_pick]] ao lado de um símbolo e depois ao lado de
outro. Toda instância do primeiro símbolo, inclusive dentro de grupos e
molduras, vira instância do segundo, e o primeiro símbolo sai do painel; os
símbolos depois dele sobem uma posição. Um Desfazer devolve tudo. Clicar no
mesmo símbolo de novo, em vez de um segundo, cancela a escolha.

## Transformar instâncias em objetos de novo {#revert}

Selecione instâncias e clique em [[docker.revert_to_objects]], ou escolha
[[menu.object > menu.object.symbol > menu.object.symbol_revert]]. Elas viram
objetos comuns que não acompanham mais o símbolo.
