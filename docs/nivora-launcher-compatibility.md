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

Executar essas versões de fato exige um backend para cada versão (ou uma
integração de runtime Java compatível com Android), além de instalação de
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
