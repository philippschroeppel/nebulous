# Why not Kubernetes?

Kubernetes is ubiquitous, but it's not the best fit for all use cases. 

__GPUs are scarce and expensive__

Where this breaks Kubernetes:

* If you need to scale up and there are no GPUs in your region, too bad.
* Running a control plane in _every_ region is expensive.
* The GPU you need may not even be in a single cloud provider, so now you are runnning a control plane in every cloud and every region :(
* If you need to connect regular services to GPU instances in different regions, Kubernetes is not made for this.
* All multi-cluster management solutions have failed.
* Need multi-tenancy in Kubernetes? LOL
* Easy to spin up locally? Eh kinda in a limited way.
* Need to secure communication to a GPU in a disparate platform or on prem? Very challenging without complex service meshes.
* Load balance traffic across clouds? Very hard.


Kubernetes was simply not designed for this use case.   

Nebulous is built from the ground up to be __cross cloud__ (native or whatever). It keeps the good parts of Kubernetes (declarative, reconciliation, composition) while making a set of primitives which fundamentally assume that resources can be running in any cloud and any region.

Nebulous hedges against the complexity of Kubernetes by making things lighter and more "batteries included".
