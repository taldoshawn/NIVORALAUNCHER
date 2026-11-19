package dev.horizon.android

import android.app.Activity
import android.content.Intent
import android.content.res.ColorStateList
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.RippleDrawable
import android.os.Build
import android.os.Bundle
import android.view.WindowInsets
import android.view.Gravity
import android.view.View
import android.widget.Button
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView

class NivoraActivity : Activity() {
    private fun dp(value: Int) = (value * resources.displayMetrics.density + .5f).toInt()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val content = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(24), dp(24), dp(24), dp(28))
            setBackgroundColor(Color.rgb(15, 13, 23))
        }
        content.addView(ImageView(this).apply {
            setImageResource(R.drawable.launcher_icon)
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, LinearLayout.LayoutParams(dp(54), dp(54)))
        content.addView(TextView(this).apply {
            text = getString(R.string.nivora_title)
            textSize = 31f
            setTypeface(typeface, Typeface.BOLD)
            setTextColor(Color.WHITE)
            setPadding(0, dp(12), 0, dp(20))
        })
        addDestination(content, R.string.nivora_play, "play", true)
        addDestination(content, R.string.nivora_accounts, "accounts")
        addDestination(content, R.string.nivora_wardrobe, "skins")
        addDestination(content, R.string.nivora_mods, "mods")
        addDestination(content, R.string.nivora_packs, "packs")
        addDestination(content, R.string.nivora_shaders, "shaders")
        addDestination(content, R.string.nivora_hud, "hud")
        addDestination(content, R.string.nivora_settings, "settings")
        setContentView(ScrollView(this).apply {
            isFillViewport = true
            addView(content)
            if (Build.VERSION.SDK_INT >= 30) {
                setOnApplyWindowInsetsListener { view, insets ->
                    val safe = insets.getInsets(WindowInsets.Type.systemBars() or WindowInsets.Type.displayCutout())
                    view.setPadding(safe.left, safe.top, safe.right, safe.bottom)
                    insets
                }
            }
        })
    }

    private fun addDestination(column: LinearLayout, label: Int, route: String, primary: Boolean = false) {
        val surface = GradientDrawable().apply {
            cornerRadius = dp(16).toFloat()
            setColor(if (primary) Color.rgb(105, 70, 203) else Color.rgb(37, 31, 53))
            setStroke(dp(1), Color.rgb(83, 66, 116))
        }
        val button = Button(this).apply {
            setText(label)
            isAllCaps = false
            textSize = 17f
            setTextColor(Color.WHITE)
            gravity = Gravity.CENTER_VERTICAL or Gravity.START
            setPadding(dp(18), 0, dp(18), 0)
            background = RippleDrawable(ColorStateList.valueOf(0x33FFFFFF), surface, null)
            setOnClickListener {
                startActivity(Intent(this@NivoraActivity, HorizonActivity::class.java)
                    .putExtra("dev.horizon.android.DESTINATION", route))
            }
        }
        column.addView(button, LinearLayout.LayoutParams(-1, dp(52)).apply {
            bottomMargin = dp(8)
        })
    }
}
