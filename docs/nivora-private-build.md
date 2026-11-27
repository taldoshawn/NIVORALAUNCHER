# Compilação privada de teste Android

O modo privado gera um aplicativo separado, com o nome **Nivora Privado** e
identificador terminado em `.private`. O atualizador público fica desativado no
Rust e a política Java existente exclui esse identificador das atualizações.
Isso evita substituir a instalação normal durante os testes. O identificador do
aplicativo não controla quem pode baixar o arquivo: a entrega também precisa
usar armazenamento restrito ao proprietário.

## Compilar

Use um commit revisado, sem alterações pendentes, Rust com o target
`aarch64-linux-android`, SDK 35, build-tools 35.0.0, NDK 28.2.13676358 e JDK 25.
As ferramentas e os insumos de compilação ficam em `.local/`, fora do Git.

```sh
python3 tools/android_build.py \
  --abi arm64-v8a --public --private-build --profile fast \
  --app-id dev.taldoshawn.nivora.legacy17.private \
  --dev-build legacy17.COMMIT --dev-version-code 1 \
  --sdk /caminho/Android/Sdk --java-home /caminho/jdk-25 \
  --ndk-version 28.2.13676358 --build-tools-version 35.0.0 \
  --gradle /caminho/gradle-8.14/bin/gradle
```

Substitua `COMMIT` pela revisão compilada. `--rust-only` permite compilar apenas
a biblioteca nativa antes de um empacotamento separado com as ferramentas do SDK.
As tabelas geradas precisam estar preparadas e passar pela lista permitida de
`generated_engine_file` antes de montar o APK.

Neste script, **`--public` seleciona o conteúdo permitido do pacote**: tabelas
geradas, sem JARs, texturas, sons ou outros recursos originais do jogo. A opção
não envia nem publica o arquivo. `--private-build` exige esse modo, uma etiqueta
de desenvolvimento, um versionCode explícito e um app ID terminado em `.private`.
O aplicativo mantém a instalação oficial dos recursos e a autenticação já
existentes; o modo privado não contorna essas verificações.

## Verificar e entregar

Execute os testes do modo privado e do atualizador:

```sh
python3 -m unittest discover -s tests -p test_android_private_build.py -v
python3 tools/test_android_updater.py
python3 tools/test_android_updater.py --private-build
```

No APK final, verifique a assinatura, ZIP alignment, identidade `.private`, nome,
arquitetura ARM64, alinhamento ELF de 16 KiB, manifesto não depurável e a auditoria
de conteúdo de `tools/check_android_release.py --public`. Registre o commit,
SHA-256, tamanho e a existência ou ausência de teste em dispositivo real.
Conserve a chave de assinatura fora do Git e fora do pacote entregue.

**Não envie um APK privado para Releases ou artefatos deste repositório público.**
Os [artefatos do GitHub Actions](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/download-workflow-artifacts)
podem ser baixados por usuários autenticados com acesso de leitura ao repositório.
Uma entrega exclusiva exige um canal cujo controle de acesso seja privado para o
proprietário. Não acione os workflows de publicação nem atualize NIVORASITE para
uma compilação de teste.

A compilação de um APK não demonstra paridade completa do motor. Registre
separadamente as versões, os cenários de jogo realmente testados e as limitações.
