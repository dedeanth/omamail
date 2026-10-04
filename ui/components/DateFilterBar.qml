import QtQuick
import qs.Commons
import qs.Ui
import "../account/DateFilter.js" as DateFilter

// Inbox date filters above the message list: All, Today, Yesterday, Last 7
// days, Last 30 days. Drawn like the mailbox chips, as one segmented control.
// Only an account whose provider can narrow its query by date shows it; the
// filter itself lives on the account, so it survives a reload of this row.
Item {
  id: root

  required property var account
  required property color textColor
  required property color accentColor

  readonly property string current: account ? String(account.dateFilter || "") : ""

  visible: !!account && account.canFilterByDate === true
  implicitWidth: track.width
  implicitHeight: visible ? track.height : 0
  height: implicitHeight

  Rectangle {
    id: track
    x: Style.space(14)
    width: chips.implicitWidth
    height: chips.implicitHeight
    radius: Style.cornerRadius
    color: "transparent"
    border.width: 1
    border.color: Style.normalBorderFor(root.textColor, root.accentColor)

    Row {
      id: chips

      Repeater {
        model: DateFilter.FILTERS

        Item {
          id: segment
          required property var modelData
          required property int index

          implicitWidth: chip.implicitWidth
          implicitHeight: chip.implicitHeight

          Rectangle {
            visible: segment.index > 0
            width: 1
            height: parent.height
            color: track.border.color
          }

          Button {
            id: chip
            objectName: "date-filter-" + (segment.modelData.key || "all")
            anchors.fill: parent
            text: segment.modelData.label
            foreground: root.textColor
            bordered: false
            selected: root.current === segment.modelData.key
            fontSize: Style.font.bodySmall
            onClicked: root.account.setDateFilter(segment.modelData.key)
          }
        }
      }
    }
  }
}
