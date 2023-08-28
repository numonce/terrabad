# Terrabad
A rust command line tool for all your proxmox vm management needs. 

---
# Usage:
Terrabad currently supports single and bulk actions of cloning and starting/stopping LXC and qemu containers. The project aims in the future to be an all encomposing solution but is currently reliant on proxmox templates.

```
terrabad.exe -h 
```
![Help](https://cdn.discordapp.com/attachments/898312220021260302/1145750472435703808/image.png)

## Examples

### Making a single clone of a template.
```
terrabad.exe --url <https://proxmox.url:8006> --user <username> --password <yourpassword> --action clone --node <yournodename> --source <VMID you wish to clone> --destination <VMID of resulting clone> --clone_type <linked/full>
```
### Making several clones of a template.
```
terrabad.exe --url <https://proxmox.url:8006> --user <username> --password <yourpassword> --action bulk_clone --node <yournodename> --source <VMID you wish to clone> --min <start of your VMID range> --max <end of your VMID range> --clone_type <linked/full> --threads <n number of threads>
```
### Starting all VMs/containers in a given range
```
terrabad.exe --url <https://proxmox.url:8006> --user <username> --password <yourpassword> --action bulk_start --node <yournodename> --min <start of your VMID range> --max <end of your VMID range>
```
## Known issues
- As of right now bulk cloning LXCs needs to be single threaded and must be a full clone. There is no built in check on threading LXCs.  
- Giving more threads to your process than what your proxmox server can handle results in some errors. Do some testing to see what is right for your configuration.


