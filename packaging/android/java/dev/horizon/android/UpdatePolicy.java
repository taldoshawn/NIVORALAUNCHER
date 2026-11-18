package dev.horizon.android;

/** Public policy only; Android remains responsible for APK signature/install verification. */
final class UpdatePolicy {
    static void check(HorizonActivity activity) {
        new Thread(() -> {
            javax.net.ssl.HttpsURLConnection connection = null;
            try {
                connection = (javax.net.ssl.HttpsURLConnection) new java.net.URL(
                    "https://nivora-launcher.vercel.app/api/app-policy").openConnection();
                connection.setConnectTimeout(5000);
                connection.setReadTimeout(5000);
                connection.setInstanceFollowRedirects(false);
                if (connection.getResponseCode() != 200) return;
                byte[] bytes;
                try (java.io.InputStream input = connection.getInputStream()) {
                    java.io.ByteArrayOutputStream out = new java.io.ByteArrayOutputStream();
                    byte[] buffer = new byte[1024]; int n;
                    while ((n=input.read(buffer)) != -1) {
                        if (out.size()+n>16384) return;
                        out.write(buffer,0,n);
                    }
                    bytes=out.toByteArray();
                }
                org.json.JSONObject policy=new org.json.JSONObject(new String(bytes,java.nio.charset.StandardCharsets.UTF_8));
                android.content.pm.PackageInfo installed=activity.getPackageManager().getPackageInfo(activity.getPackageName(),0);
                long current=android.os.Build.VERSION.SDK_INT>=28?installed.getLongVersionCode():installed.versionCode;
                long minimum=policy.getLong("minimumVersionCode"),latest=policy.getLong("latestVersionCode");
                if(minimum<0 || latest<minimum || latest<=current) return;
                boolean required=current<minimum;
                String message=policy.optString("message", "Uma atualização do NIVORA está disponível.");
                activity.runOnUiThread(() -> {
                    if(activity.isFinishing() || activity.isDestroyed()) return;
                    android.app.AlertDialog.Builder dialog=new android.app.AlertDialog.Builder(activity)
                        .setTitle(required?"Atualização obrigatória":"Atualização disponível")
                        .setMessage(message).setCancelable(!required)
                        .setPositiveButton("Atualizar",(d,w)->{
                            activity.startActivity(new android.content.Intent(android.content.Intent.ACTION_VIEW,
                                android.net.Uri.parse("https://nivora-launcher.vercel.app/downloads")));
                            if(required) activity.finish();
                        });
                    if(!required) dialog.setNegativeButton("Agora não",null);
                    else dialog.setNegativeButton("Sair",(d,w)->activity.finish());
                    dialog.show();
                });
            } catch(Exception ignored) {
                // An outage must not disable the community's installations.
            } finally { if(connection!=null) connection.disconnect(); }
        },"NivoraPolicy").start();
    }
}
