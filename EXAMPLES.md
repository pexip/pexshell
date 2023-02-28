# Pexshell Examples

## Listing all current conferences with their participants

List all currently active participants:

```bash
pexshell status participant get
```

For more advanced formatting and filtering the output can be piped into [jq](https://stedolan.github.io/jq/).
For example, to produce each participant's display name and conference in a tab-separated list:

```bash
pexshell status participant get | jq -r '.[] | [.display_name, .conference] | @tsv'
```

Example output:

```plaintext
John    Mark VMR
Luke    Matt VMR
Mark    Mark VMR
Matt    Matt VMR
```

We can improve the readability of the output further.
The following bash script uses pexshell to list all currently active conferences with the number of participants present in them.
Conferences with fewer than 16 participants will have their display names listed below each conference.

```bash
#!/usr/bin/env bash

readarray -t conferences < <(pexshell status participant get | jq -c 'map({conference, display_name, source_alias}) | group_by(.conference) | map({conference: .[0].conference, participants: [.[].display_name]}) | sort_by(.participants | length) | reverse | .[]')

for conference in "${conferences[@]}"; do
    conf_name="$(echo $conference | jq -r .conference)"
    readarray -t participants < <(echo "$conference" | jq -r '.participants | .[]')
    num_participants=${#participants[@]}
    echo "$conf_name  ($num_participants participants)"
    if [ $num_participants -lt 16 ]; then
        for participant in "${participants[@]}"; do
            echo "        $participant"
        done
    fi
done
```

Example output:

```plaintext
Large Conference  (32 participants)
Test Conference  (4 participants)
        Jack
        James
        Jill
        John
Amy VMR  (3 participants)
        Adam
        Amy
        Allen
Luke VMR (2 participants)
        Luke
        Liam
```

## Creating a Virtual Meeting Room

```bash
pexshell configuration conference post --name "New VMR" --service_type "conference"
```

## Changing an existing Virtual Meeting Room

The following example updates the PIN of the Virtual Meeting Room with ID 1 to 1234:

```bash
pexshell configuration conference patch 1 --pin "1234"
```

### Daily cron job to change VMR pin

Using pexshell to run a cron job to update your a VMR’s pin to something you know.

```bash
0 10 * * * pexshell configuration conference patch $(pexshell configuration conference get --name="My VMR" | jq '.[0].id') --pin=1234
```

## Deleting a Virtual Meeting Room

The following example deletes the VMR with ID 1:

```bash
pexshell configuration conference delete 1
```

## Turning off crash reporting

Pexshell can be used to switch on/off error reporting for an instance.

```bash
pexshell configuration global patch 1 --error_reporting_enabled false
```

## Calculating total call capacity

Pexshell can be used to calculate the total call capacity of an Infinity deployment over all of the conferencing nodes. We can easily produce a JSON blob with this data.

```bash
pexshell status worker_vm get | jq '[.[] | {max_sd_calls, max_hd_calls, max_full_hd_calls}] | add'
```

Example output:

```json
{
  "max_sd_calls": 52,
  "max_hd_calls": 22,
  "max_full_hd_calls": 12
}
```

## Checking if conferencing nodes are (un)reachable

Pexshell can be used to retrieve IP addresses of worker nodes, which is useful for performing liveness tests.
For example, you could use this to ping all conferencing nodes in a deployment.

```bash
pexshell configuration worker_vm get | jq -r '.[] | .address' | xargs -n1 ping -w 2
```

Alternatively, we can use the following script for more readable results:

```bash
#!/usr/bin/env bash

nodes="$(pexshell configuration worker_vm get | jq -r '.[].address')"
for n in $nodes; do\
    ping -w 1 -i 0.2 "$n" && success=$success$'\n'"$n" || failure=$failure$'\n'"$n"
done
echo "Successfully contacted:" $success
echo "     Failed to contact:" $failure
```

## Using with powershell

Pexshell currently doesn't have a powershell-native module, but can still be used effectively from powershell in combination with the `ConvertFrom-Json` cmdlet.
For example, we can list all aliases of a conference like so:

```powershell
PS > (pexshell configuration conference get 64 | ConvertFrom-Json).aliases.alias
test_call
test_call@infinity.pexip.com
```

If you didn't know the conference ID, you could instead use the conference name:

```powershell
PS > (pexshell configuration conference get --name="My VMR" | ConvertFrom-Json)[0].aliases.alias
my.vmr
my.vmr@infinity.pexip.com
```

### Listing all current conferences with their participants

```powershell
PS > pexshell status participant get | ConvertFrom-Json | Format-Table display_name,conference

display_name conference
------------ ----------
John         Mark VMR
Luke         Matt VMR
Mark         Mark VMR
Matt         Matt VMR
```

Or even better:

```powershell
PS > pexshell status participant get | ConvertFrom-Json | Sort-Object conference | Format-Table call_uuid,display_name,source_alias -GroupBy conference

   conference: conf.test

id                                   display_name source_alias
--                                   ------------ ------------
33f583c5-fa14-44e5-a1e3-8433359130ca Jack         Jack
9ceddcfe-d848-4938-8bbd-b9a8e21692bd James        James
f2e29770-6965-4f6a-9b19-dd20d76b1665 Jill         Jill

   conference: conf.ace

id                                   display_name source_alias
--                                   ------------ ------------
86d86427-fa3b-4f62-a73b-274ff7932d7d John         john@infinity.pexip.com

   conference: conf.example

id                               display_name source_alias
--                               ------------ ------------
27ffee4fec6f093c58150e63bc40b5a2 Adam         sip:adam@infinity.pexip.com
```

### Turning off crash reporting

```powershell
PS > pexshell configuration global patch 1 --error_reporting_enabled $false
```

### Calculating total call capacity

```powershell
PS > pexshell status worker_vm get | ConvertFrom-Json | Measure-Object -Property max_sd_calls,max_hd_calls,max_full_hd_calls -Sum | Format-Table Property,Sum

Property          Sum
--------          ---
max_sd_calls       94
max_hd_calls       39
max_full_hd_calls  22
```

### Checking if conferencing nodes are (un)reachable

```powershell
$nodes=pexshell configuration worker_vm get | ConvertFrom-Json
foreach ($n in $nodes) { ping -w 2 $n.address }
```

### Change active conference layout to 4:0

```powershell
PS > pexshell command conference transform_layout --conference_id 9a579efb-f1b5-4570-a718-b436d4154b0f --layout 4:0
```

### CSV export for Excel

The following is a translation of [this](https://docs.pexip.com/api_manage/extract_analyse.htm) example. Pexshell significantly simplifies the script:

```powershell
$now = Get-Date

# Convert the current time to a sortable format (suits the Management Node):
$pexNow = Get-Date $now -Format s

# Number of days ago to start the report from:
$start = $now.AddDays(-1)

# Convert the start time to a sortable format (suits the Management Node):
$start = Get-Date $start -Format s

$participants = pexshell history participant get --limit=5000 --end_time__gte=$start --end_time__lt=$pexNow | ConvertFrom-Json

$conferences = pexshell history conference get --limit=5000 --end_time__gte=$start --end_time__lt=$pexNow | ConvertFrom-Json

Write-Output $participants | Export-Csv -Path "$((Get-Date).ToString('yyyy-MM-dd_hh-mm-ss'))_pexHistoryPart.csv" -Delimiter "," -NoTypeInformation
Write-Output $participants | Export-csv -Append pexHistoryPart.csv -Delimiter "," -NoTypeInformation #write the csv file to the same dir as where this script is run from

Write-Output $conferences | Export-Csv -Path "$((Get-Date).ToString('yyyy-MM-dd_hh-mm-ss'))_pexHistoryConf.csv" -Delimiter "," -NoTypeInformation
Write-Output $conferences | Export-csv -Append pexHistoryConf.csv -Delimiter "," -NoTypeInformation #write the csv file to the same dir as where this script is run from
```
