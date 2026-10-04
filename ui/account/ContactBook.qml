import QtQuick

// Recipient suggestions. Local sources come from the backend's contacts.suggest
// (Thunderbird and Betterbird profiles, contacts.json/.vcf and cached mail),
// read without modifying them. Each Gmail account adds its Google Contacts
// (API 7), fetched at most every half hour: the address book changes slowly
// and each refresh is a few People API pages. A grant without the optional
// contacts scopes answers with an empty list.
QtObject {
  id: root

  required property var service
  property var contacts: []
  property bool loading: false
  property var local: []
  property var google: ({})
  property var fetched: ({})
  readonly property int maxAge: 30 * 60 * 1000

  function refresh() {
    var backend = service.backend
    if (loading || !backend || !backend.ready) return
    loading = true
    backend.call("contacts.suggest", {}, function(result, error) {
      root.loading = false
      if (!error && Array.isArray(result)) {
        root.local = result
        root.merge()
      }
    })
    if (!service.backendCanSuggestGoogleContacts) return
    var accounts = service.accountList ? service.accountList.accounts : []
    for (var i = 0; i < accounts.length; i++) {
      if (accounts[i].provider === "gmail") refreshGoogle(accounts[i].id)
    }
  }

  function refreshGoogle(accountId) {
    if (Date.now() - (fetched[accountId] || 0) < maxAge) return
    var stamped = Object.assign({}, fetched)
    stamped[accountId] = Date.now()
    fetched = stamped
    service.backend.call("contacts.google", { accountId: accountId }, function(result, error) {
      if (error || !result || !Array.isArray(result.contacts)) {
        // Signed out or offline: try again on the next refresh.
        var retry = Object.assign({}, root.fetched)
        delete retry[accountId]
        root.fetched = retry
        return
      }
      var next = Object.assign({}, root.google)
      next[accountId] = result.contacts
      root.google = next
      root.merge()
    })
  }

  // Local sources first (they carry the names Thunderbird learned), then each
  // account's Google Contacts; one row per address.
  function merge() {
    var seen = ({})
    var merged = []
    function add(list) {
      for (var i = 0; i < list.length; i++) {
        var key = String(list[i].email || "").toLowerCase()
        if (key === "" || seen[key]) continue
        seen[key] = true
        merged.push(list[i])
      }
    }
    add(local)
    for (var account in google) add(google[account])
    contacts = merged
  }
}
