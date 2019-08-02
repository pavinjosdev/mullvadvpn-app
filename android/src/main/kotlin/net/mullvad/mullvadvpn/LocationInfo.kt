package net.mullvad.mullvadvpn

import android.content.Context
import android.view.View
import android.widget.TextView

import net.mullvad.mullvadvpn.model.Endpoint
import net.mullvad.mullvadvpn.model.GeoIpLocation
import net.mullvad.mullvadvpn.model.TransportProtocol
import net.mullvad.mullvadvpn.model.TunnelState

class LocationInfo(val parentView: View, val context: Context) {
    private val country: TextView = parentView.findViewById(R.id.country)
    private val city: TextView = parentView.findViewById(R.id.city)
    private val tunnelInfo: View = parentView.findViewById(R.id.tunnel_info)
    private val hostname: TextView = parentView.findViewById(R.id.hostname)
    private val chevron: View = parentView.findViewById(R.id.chevron)
    private val protocol: TextView = parentView.findViewById(R.id.tunnel_protocol)
    private val inAddress: TextView = parentView.findViewById(R.id.in_address)
    private val outAddress: TextView = parentView.findViewById(R.id.out_address)

    private var endpoint: Endpoint? = null
    private var isTunnelInfoVisible = false
    private var isTunnelInfoExpanded = false

    var location: GeoIpLocation? = null
        set(value) {
            country.text = value?.country ?: ""
            city.text = value?.city ?: ""
            hostname.text = value?.hostname ?: ""

            updateOutAddress(value)
        }

    var state: TunnelState = TunnelState.Disconnected()
        set(value) {
            field = value

            when (value) {
                is TunnelState.Connecting -> {
                    endpoint = value.endpoint?.endpoint
                    isTunnelInfoVisible = true
                }
                is TunnelState.Connected -> {
                    endpoint = value.endpoint.endpoint
                    isTunnelInfoVisible = true
                }
                else -> {
                    endpoint = null
                    isTunnelInfoVisible = false
                }
            }

            updateTunnelInfo()
        }

    init {
        tunnelInfo.setOnClickListener { toggleTunnelInfo() }
    }

    private fun toggleTunnelInfo() {
        isTunnelInfoExpanded = !isTunnelInfoExpanded
        updateTunnelInfo()
    }

    private fun updateTunnelInfo() {
        if (isTunnelInfoVisible) {
            showTunnelInfo()
        } else {
            hideTunnelInfo()
        }
    }

    private fun hideTunnelInfo() {
        chevron.visibility = View.INVISIBLE

        protocol.text = ""
        inAddress.text = ""
        outAddress.text = ""
    }

    private fun showTunnelInfo() {
        chevron.visibility = View.VISIBLE

        if (isTunnelInfoExpanded) {
            chevron.rotation = 180.0F
            protocol.setText(R.string.wireguard)
            showInAddress(endpoint)
            updateOutAddress(location)
        } else {
            chevron.rotation = 0.0F
            protocol.text = ""
            inAddress.text = ""
            outAddress.text = ""
        }
    }

    private fun showInAddress(endpoint: Endpoint?) {
        if (endpoint != null) {
            val transportProtocol = when (endpoint.protocol) {
                is TransportProtocol.Tcp -> context.getString(R.string.tcp)
                is TransportProtocol.Udp -> context.getString(R.string.udp)
            }

            inAddress.text = context.getString(
                R.string.in_address,
                endpoint.address.address.hostAddress,
                endpoint.address.port,
                transportProtocol
            )
        } else {
            inAddress.text = ""
        }
    }

    private fun updateOutAddress(location: GeoIpLocation?) {
        val addressAvailable = location != null && (location.ipv4 != null || location.ipv6 != null)

        if (isTunnelInfoVisible && addressAvailable && isTunnelInfoExpanded) {
            val ipv4 = location!!.ipv4
            val ipv6 = location.ipv6
            val ipAddress: String

            if (ipv6 == null) {
                ipAddress = ipv4!!.hostAddress
            } else if (ipv4 == null) {
                ipAddress = ipv6.hostAddress
            } else {
                ipAddress = "${ipv4.hostAddress} / ${ipv6.hostAddress}"
            }

            outAddress.text = context.getString(R.string.out_address, ipAddress)
        } else {
            outAddress.text = ""
        }
    }
}
