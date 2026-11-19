plugins {
    id("com.android.application") version "8.11.1"
    id("org.jetbrains.kotlin.android") version "2.2.21"
}

val releaseKeyStore = System.getenv("HORIZON_ANDROID_KEYSTORE")
val releaseKeyStorePassword = System.getenv("HORIZON_ANDROID_KEYSTORE_PASSWORD")
val releaseKeyAlias = System.getenv("HORIZON_ANDROID_KEY_ALIAS")
val releaseKeyPassword = System.getenv("HORIZON_ANDROID_KEY_PASSWORD")
val hasReleaseKey = listOf(releaseKeyStore, releaseKeyStorePassword, releaseKeyAlias, releaseKeyPassword)
    .all { !it.isNullOrEmpty() }
val baseVersion = "0.1.0"
val devVersionCode = providers.gradleProperty("devVersionCode")
val devBuild = providers.gradleProperty("devBuild")

android {
    namespace = "dev.horizon.android"
    compileSdk = 35
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    defaultConfig {
        val verifySigner = hasReleaseKey || System.getenv("HORIZON_VERIFY_OFFICIAL_SIGNER") == "1"
        buildConfigField("boolean", "VERIFY_OFFICIAL_SIGNER", verifySigner.toString())
        applicationId = providers.gradleProperty("appId").getOrElse("com.goydevv.horizon")
        minSdk = providers.gradleProperty("minSdk").getOrElse("28").toInt()
        targetSdk = 35
        versionCode = devVersionCode.getOrElse("30").toInt()
        versionName = if (devVersionCode.isPresent) "$baseVersion-dev.${devBuild.get()}" else baseVersion
        ndk { abiFilters.add(providers.gradleProperty("abi").get()) }
    }
    buildFeatures { buildConfig = true }
    signingConfigs {
        create("development") {
            storeFile = file("../../.local/android/debug.keystore")
            storePassword = "android"
            keyAlias = "androiddebugkey"
            keyPassword = "android"
        }
        if (hasReleaseKey) create("releaseKey") {
            storeFile = file(requireNotNull(releaseKeyStore))
            storePassword = releaseKeyStorePassword
            keyAlias = releaseKeyAlias
            keyPassword = releaseKeyPassword
        }
    }
    buildTypes {
        getByName("debug") {
            signingConfig = signingConfigs.getByName("development")
            versionNameSuffix = "-dev.${devBuild.getOrElse("local")}"
            if (providers.gradleProperty("useDevSuffix").isPresent) applicationIdSuffix = ".dev"
        }
        getByName("release") {
            isDebuggable = false
            isJniDebuggable = false
            isMinifyEnabled = false
            signingConfig = signingConfigs.getByName(if (hasReleaseKey) "releaseKey" else "development")
        }
    }
    sourceSets.getByName("main") {
        java.setSrcDirs(listOf("java", "kotlin"))
        manifest.srcFile("AndroidManifest.xml")
        jniLibs.setSrcDirs(listOf("../../.local/android/gradle-jni"))
        assets.setSrcDirs(listOf("../../.local/android/gradle-assets"))
        res.setSrcDirs(listOf("res"))
    }
    packaging {
        jniLibs {
            useLegacyPackaging = true
            keepDebugSymbols.add("**/*.so")
        }
    }
}
layout.buildDirectory.set(file("../../.local/android/gradle-build"))
