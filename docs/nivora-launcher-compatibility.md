# Nivora: home, shaders e catálogo Modrinth

O app Android usa o cliente Rust do projeto, com recursos e mundos do motor
26.3. Selecionar outra versão de rede não instala o cliente Java dessa versão.

## Alterações

- A home mostra o estado real da instalação, versão, perfil e atalhos para
  Modrinth, shaders e multiplayer/LAN. Os números de mods e shaders vêm do perfil.
- A origem de mods escolhida é preservada ao reiniciar. A busca e a paginação
  usam Modrinth quando selecionado, sem inserir os mods internos nos resultados.
- A falha de adaptação de um JAR não cria uma entrada de instalação bem-sucedida.
  Mods do Modrinth continuam sujeitos às capacidades do adaptador nativo; baixar
  um JAR não equivale a executá-lo com Forge ou Fabric.
- O desenho antecipado de sol e lua usa a projeção de céu já usada pelo desenho
  tardio. O recorte de distância do terreno deixa de cortar esses corpos.
  O teste de regressão usa o WGSL real e compara pixels com quatro planos de
  recorte. Ele não comprova compatibilidade com todos os shaders e drivers Android.

## Multiplayer offline

O atalho **Jogue com amigos** reúne perfil local, IP:porta e abertura do jogo.
O transporte LAN/offline já existe no motor; o atalho facilita seu uso.

1. Conecte os aparelhos à mesma rede Wi-Fi e escolha nomes locais diferentes.
2. No aparelho anfitrião, abra um mundo e use a opção de abrir para LAN.
3. No outro aparelho, entre em Multiplayer e use o IP e a porta exibidos pelo
   anfitrião. Também é possível salvar IP:porta pelo atalho na home.

O servidor precisa aceitar perfis offline. Servidores autenticados continuam
exigindo uma conta com Minecraft Java. Offline significa sem autenticação online;
o multiplayer ainda precisa de conectividade entre os aparelhos.

## Versões

| Versão | Situação no Android |
| --- | --- |
| 26.3 | Motor de mundos locais e multiplayer existente |
| 1.8–1.8.9 | Transporte multiplayer de protocolo 47 experimental |
| 1.8.9 singleplayer | Não implementado; não abre mundos antigos no motor moderno |
| 1.7.10, 1.12.2, 1.20.1 | Clientes/motores e protocolos ainda não implementados |

A escolha do projeto é manter somente Rust. Executar essas versões de fato
exige portar as regras, os registros, a geração de mundo e os saves de cada
versão para o motor nativo, além de instalação de
recursos, isolamento de mundos e validação de controles e renderização. Esta
alteração não adiciona entradas de versão que iniciariam outro jogo.

## Verificação direcionada

```sh
python3 tools/test_android_celestial.py
python3 tools/test_android_launch.py
python3 tools/check_public_tree.py
```

O teste celestial precisa de um adaptador Vulkan, que pode ser Mesa lavapipe.
Não inicia o jogo nem acessa dados de jogadores.

## Desempenho e memória

Em Configurações → Vídeo, o seletor oferece 30, 45, 60, 90, 120, 144, 165,
240 FPS e Sem limite. Internamente 260 é o sentinela de **sem limite**, não
uma promessa de alcançar 260 FPS. VSync desligado mantém os limites escolhidos.
O jogo volta a reduzir FPS em menus, ao minimizar e quando o modo bateria exige.
O launcher permanece com VSync e espera entre frames mesmo em modo alto FPS,
evitando consumir CPU continuamente antes de entrar no mundo.

Os presets agora são aplicados quando selecionados. Iniciar o jogo não volta a
sobrescrever FPS, VSync, qualidade e distâncias personalizadas. A opção Auto da
tela deixa o Android escolher a frequência em vez de solicitar sempre 120 Hz.

Em Configurações → Launcher:

- **Alvo de RAM do processo**: opcional, 512–8192 MiB. Mede VmRSS do processo
  a cada dois segundos. A partir de 90% do alvo reduz dois chunks de visão por
  amostra até o mínimo de dois. Abaixo de 75%, recupera um chunk a cada cinco
  amostras consecutivas. Informa o servidor da nova distância; preserva as
  distâncias salvas e a simulação. Não é limite rígido nem cobre memória da GPU.
  Mundos, simulação e mods podem manter o processo acima do alvo mínimo.
- **Limite do cache de memória**: 16–1024 MiB, aplicado aos caches descartáveis
  de sons e buffers de malha vazios. Não limita todas as alocações do processo.
- **Chunk worker thread ceiling** limita os workers de geração/malha, sem
  forçar o Android a criar mais workers que o modo automático considera adequado.

Não há heap Java ou parâmetro -Xmx. Não foram medidos ganhos de FPS em celular.
O relatório local inclui alvo de RAM, última leitura RSS e visão efetiva para
permitir comparar sessões no aparelho.

### Perfil para aparelhos de 2 GB

Na primeira abertura desta atualização, aparelhos com MemTotal de até 2304 MiB
recebem um ponto de partida leve: visão 4, simulação 5, escala 50%, FPS sem limite,
cache 16 MiB, um worker, alvo de RSS 768 MiB, LOD e shaders desligados.
HDR e antialiasing temporal ficam desligados, e as texturas de blocos usam
orçamento de 16 MiB no nível principal (mipmaps e o mínimo de 16 px ficam fora
desse orçamento). O atlas de interface é limitado a 4096 px, usando o redimensionamento móvel
existente para texturas não essenciais. A resolução do terreno não é alterada
por esse atlas. O launcher também respeita um prazo entre redraws.

O botão **Aplicar perfil leve** em Configurações → Launcher reaplica o perfil.
O atlas precisa de nova abertura do app; as outras opções aplicam ao jogar.
A detecção usa RAM física, não memória disponível momentânea, e ocorre somente
uma vez: escolhas posteriores são preservadas. Mods e shaders continuam
selecionáveis, mas podem exceder a capacidade desses aparelhos. O alvo de
768 MiB não garante sobrevivência ao gerenciador de memória do Android.

### FPS sem limite por padrão

O perfil leve também usa **Sem limite**, com VSync e modo bateria desligados.
260 é somente o sentinela interno: o limitador não espera o prazo de 260 FPS.
Os perfis existentes recebem essa configuração uma vez nesta atualização,
inclusive ao trocar de perfil. Depois, escolhas manuais de FPS/VSync são
preservadas. A redução de FPS em menus/minimização continua independente; a
frequência física da tela e as capacidades do driver não são garantias de FPS.
