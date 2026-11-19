package dev.horizon.android

import android.app.Activity
import android.content.Intent
import android.content.res.Configuration
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
import android.widget.FrameLayout
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
        val banner = FrameLayout(this).apply {
            background = GradientDrawable().apply {
                cornerRadius = dp(20).toFloat()
                setColor(Color.rgb(37, 31, 53))
            }
            clipToOutline = true
        }
        banner.addView(ImageView(this).apply {
            setImageResource(R.drawable.nivora_banner)
            scaleType = ImageView.ScaleType.CENTER_CROP
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, FrameLayout.LayoutParams(-1, -1))
        banner.addView(View(this).apply {
            background = GradientDrawable(
                GradientDrawable.Orientation.BOTTOM_TOP,
                intArrayOf(0xE90F0D17.toInt(), 0x240F0D17)
            )
        }, FrameLayout.LayoutParams(-1, -1))
        banner.addView(TextView(this).apply {
            text = getString(R.string.nivora_title)
            textSize = 28f
            setTypeface(typeface, Typeface.BOLD)
            setTextColor(Color.WHITE)
            setPadding(dp(18), dp(12), dp(18), dp(16))
            if (Build.VERSION.SDK_INT >= 28) isAccessibilityHeading = true
        }, FrameLayout.LayoutParams(-1, -2, Gravity.BOTTOM))
        content.addView(banner, LinearLayout.LayoutParams(-1, dp(120)).apply {
            bottomMargin = dp(14)
        })
        addDestination(content, R.string.nivora_play, "play", true)
        val destinations = listOf(
            R.string.nivora_accounts to "accounts",
            R.string.nivora_wardrobe to "skins",
            R.string.nivora_mods to "mods",
            R.string.nivora_packs to "packs",
            R.string.nivora_shaders to "shaders",
            R.string.nivora_hud to "hud",
            R.string.nivora_settings to "settings"
        )
        val compact = resources.configuration.screenWidthDp < 560 ||
            resources.configuration.orientation == Configuration.ORIENTATION_PORTRAIT
        for (group in destinations.chunked(if (compact) 1 else 2)) {
            val row = LinearLayout(this)
            content.addView(row, LinearLayout.LayoutParams(-1, -2))
            for ((label, route) in group) addDestination(row, label, route, inRow = true)
        }
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

    private fun addDestination(
        container: LinearLayout, label: Int, route: String,
        primary: Boolean = false, inRow: Boolean = false
    ) {
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
        val layout = if (inRow) LinearLayout.LayoutParams(0, -2, 1f)
            else LinearLayout.LayoutParams(-1, -2)
        button.minHeight = dp(52)
        layout.bottomMargin = dp(8)
        if (inRow && container.childCount > 0) layout.marginStart = dp(8)
        container.addView(button, layout)
    }
}
