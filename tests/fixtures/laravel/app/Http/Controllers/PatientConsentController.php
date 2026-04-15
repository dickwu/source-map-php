<?php

namespace App\Http\Controllers;

use App\Services\ConsentService;

class PatientConsentController
{
    /**
     * Store signed patient consent.
     */
    public function store(string $patientId): bool
    {
        $service = new ConsentService();
        return $service->sign($patientId);
    }
}
