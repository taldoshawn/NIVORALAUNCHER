# Multiplayer 1.8.9 no Android

Selecione **1.8.9** no launcher antes de iniciar. Essa opção usa o protocolo
47 no cliente nativo Rust. O renderizador continua usando os recursos modernos
preparados pelo aplicativo; isso não equivale a executar o Minecraft Java 1.8.9
ou os mods Forge/OptiFine.

## Mush: validação da conexão

Se `jogar.mush.com.br` responder que precisa validar sua conexão, abra
**https://mush.com.br/validar** no navegador do celular e siga as instruções do
próprio servidor usando o mesmo nick selecionado no aplicativo. Depois volte ao
jogo e tente conectar novamente. O aplicativo não conclui nem contorna essa
validação. Outros servidores podem exigir uma conta Microsoft que tenha o jogo.

A mensagem de recusa deve mostrar o texto do servidor, incluindo o endereço
de validação, em vez de um objeto JSON com `extra`, `color` e `clickEvent`.
Uma recusa no login também deve interromper as tentativas automáticas de
reconexão quando o servidor exigir uma ação do jogador.

## Erro `1.8.9 packet 0x26: unknown block name`

Essa mensagem identifica uma falha do cliente ao converter blocos de chunks
recebidos do servidor. A investigação encontrou 102 estados incompatíveis com
a tabela do APK anterior: troncos com casca, folhas, spawners, portais, melões e
cabeças. A correção começa as atualizações de nomes na versão de dados correta
e converte as propriedades das folhas e os marcadores intermediários das
cabeças. Nenhum bloco desconhecido é substituído indiscriminadamente por ar.

O teste `legacy_registry` decodifica pacotes bulk `0x26` contendo todos os
4.096 estados numéricos, com e sem skylight, e resolve cada estado contra o
`facts.json` usado pelo APK. A compilação deve passar nesse teste antes de
empacotar o aplicativo. As informações específicas de cabeças (tipo, rotação
e skin de jogador), enviadas separadamente em block entities, ainda precisam
de integração; o estado numérico usa uma cabeça de esqueleto com a fixação
correta. Esse limite visual não deve derrubar a conexão.

## Conexão encerrada ou tempo esgotado

`Connection closed by the server` significa que a conexão foi fechada sem uma
mensagem de recusa. `Timed out` significa que o cliente não recebeu dados dentro
do prazo. Essas mensagens, sozinhas, não comprovam erro no protocolo: podem
ocorrer por recusa/proteção do servidor ou por problemas de rede. Conclua a
validação quando solicitada e preserve a mensagem e os logs de uma nova tentativa
se o problema continuar. Não compartilhe tokens de conta ou senhas nos logs.

O teste local com Cuberite cobre entrada no mundo, movimento, colocar/quebrar
blocos e um ataque contra um segundo cliente de teste. Ele não comprova acesso
ao Mush, a todos os seus modos de jogo ou compatibilidade completa com anti-cheat.
